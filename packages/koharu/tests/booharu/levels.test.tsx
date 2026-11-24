import { QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { TitleBar } from '@/components/app/TitleBar'
import { LevelsDialog } from '@/components/booharu/levels/LevelsDialog'
import { ActivityCenter } from '@/components/editor/ActivityCenter'
import { PageRail } from '@/components/editor/PageRail'
import { PipelinePreferences } from '@/components/preferences/PipelinePreferences'
import {
  closeLevels,
  completeLevels,
  menuPages,
  openLevels,
  skipReason,
  useLevels,
} from '@/lib/booharu/levels'
import i18n from '@/lib/i18n'
import { pageKey, projectKey, queryClient, type ProjectView } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  events,
  type LevelsProgress,
  type LevelsReport,
  type LevelsSkip,
  type Page,
  type PageId,
} from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

import { builtInLevels, builtInLettering, builtInReading, settingsDesktop } from './settings'

const pageIds = ['p1', 'p2', 'p3']

/** The project open with three pages, the canvas showing the first. */
function openProject(name = 'Demo'): void {
  const size = { width: 800, height: 1200 }
  const project: ProjectView = {
    name,
    page: 'p1',
    pages: pageIds.map((id) => ({ id, size, background: null, texts: 0, translated: 0 })),
    history: { steps: [], current: 0 },
    revisions: {},
  }
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pageKey, { id: 'p1', size, background: null, layers: [] } as Page)
}

/** A promise and its two ends, as a command still running gives it. */
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}

/** The desktop, with the pages in `originals` levelled before, each keeping its Original scan; a
 * fix that goes on until `finish` or `fail` ends it; and the progress it tells, through `progress`. */
function levelsDesktop(originals: PageId[] = []) {
  const settings = settingsDesktop()
  const status = vi
    .spyOn(commands, 'booharuGetLevelsStatus')
    .mockImplementation(async (pages) =>
      pages.map((page) => ({ page, original: originals.includes(page), blocked: null })),
    )
  const preview = vi
    .spyOn(commands, 'booharuPreviewLevels')
    .mockImplementation(async (page, levels) => ({
      black: 12,
      white: 243,
      before: btoa(`before ${page}`),
      after: btoa(`after ${page} ${JSON.stringify(levels)}`),
    }))
  let fixing = deferred<LevelsReport>()
  const fix = vi.spyOn(commands, 'booharuFixPageLevels').mockImplementation(() => fixing.promise)
  const stop = vi.spyOn(commands, 'booharuStopLevels').mockResolvedValue(null)
  const restore = vi
    .spyOn(commands, 'booharuRestoreOriginalPages')
    .mockImplementation(async (pages) => ({ changed: pages, skipped: [] }))
  // The Edit menu's box items read the page's split balloons; it has none.
  vi.spyOn(commands, 'booharuFindSplitBalloons').mockResolvedValue([])
  let told: ((progress: LevelsProgress) => void) | null = null
  vi.spyOn(events, 'booharuLevelsProgress').mockImplementation(async (handler) => {
    told = handler
    return () => undefined
  })
  return {
    ...settings,
    status,
    preview,
    fix,
    stop,
    restore,
    /** Ends the fix going with `report`. */
    finish(report: LevelsReport) {
      act(() => fixing.resolve(report))
      fixing = deferred()
    },
    /** Ends the fix going with `error`, as a refusal or a stop does. */
    fail(error: unknown) {
      act(() => fixing.reject(error))
      fixing = deferred()
    },
    /** Tells how far the fix going is, as the desktop does after each page. */
    progress(progress: LevelsProgress) {
      act(() => told?.(progress))
    },
  }
}

/** `ui`, with the levels dialog beside it as the editor mounts it. */
function mount(ui: ReactNode) {
  return render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delay={0}>
        {ui}
        <LevelsDialog />
      </TooltipProvider>
    </QueryClientProvider>,
  )
}

/** The open levels dialog. */
function dialog() {
  return screen.findByRole('dialog', { name: 'Fix page levels' })
}

/** The dialog's Apply, once the saved values are read. */
async function apply() {
  const button = within(await dialog()).getByRole('button', { name: 'Apply' })
  await waitFor(() => expect(button).toBeEnabled())
  return button
}

/** The page menu of the page named `name`, open. */
async function pageMenu(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(await screen.findByRole('button', { name: `Actions for ${name}` }))
  return screen.findByRole('menu')
}

beforeEach(() => {
  openProject()
  closeLevels()
})

afterEach(() => {
  cleanup()
  document.body.replaceChildren()
})

describe('Fix page levels: the pages it takes', () => {
  it("acts on the selection when a page's menu is for one of its pages, else on that page", () => {
    expect(menuPages('p3', ['p3', 'p1'])).toEqual(['p1', 'p3'])
    expect(menuPages('p2', ['p3', 'p1'])).toEqual(['p2'])
    expect(menuPages('p2', [])).toEqual(['p2'])
  })

  it("opens from a page's menu on the selected pages, in the project's order", async () => {
    const { fix } = levelsDesktop()
    const user = userEvent.setup()
    useKoharuStore.setState({ selectedPages: ['p3', 'p1'] })
    mount(<PageRail />)

    await user.click(
      within(await pageMenu(user, 'Page 1')).getByRole('menuitem', { name: 'Fix page levels…' }),
    )
    const shown = await dialog()
    expect(within(shown).getByRole('radio', { name: 'Selected pages' })).toBeChecked()
    await user.click(await apply())
    expect(fix).toHaveBeenCalledExactlyOnceWith(['p1', 'p3'], builtInLevels)
  })

  it('opens from the menu of a page outside the selection on that page alone', async () => {
    const { fix } = levelsDesktop()
    const user = userEvent.setup()
    useKoharuStore.setState({ selectedPages: ['p3', 'p1'] })
    mount(<PageRail />)

    await user.click(
      within(await pageMenu(user, 'Page 2')).getByRole('menuitem', { name: 'Fix page levels…' }),
    )
    await user.click(await apply())
    expect(fix).toHaveBeenCalledExactlyOnceWith(['p2'], builtInLevels)
  })

  it('opens from the Edit menu on every page, with the selection one choice away', async () => {
    const { fix } = levelsDesktop()
    const user = userEvent.setup()
    mount(<TitleBar />)

    await user.click(screen.getByRole('menuitem', { name: 'Edit' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Fix page levels…' }))
    const shown = await dialog()
    expect(within(shown).getByRole('radio', { name: 'All pages' })).toBeChecked()
    // Nothing selected in the page list: there are no selected pages to fix.
    expect(within(shown).getByRole('radio', { name: 'Selected pages' })).toHaveAttribute(
      'aria-disabled',
      'true',
    )
    await user.click(await apply())
    expect(fix).toHaveBeenCalledExactlyOnceWith(pageIds, builtInLevels)
  })

  it('fixes the selection from the Edit menu once Selected pages is chosen', async () => {
    const { fix, finish } = levelsDesktop()
    const user = userEvent.setup()
    useKoharuStore.setState({ selectedPages: ['p2'] })
    mount(<TitleBar />)

    await user.click(screen.getByRole('menuitem', { name: 'Edit' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Fix page levels…' }))
    const shown = await dialog()
    await user.click(within(shown).getByRole('radio', { name: 'Selected pages' }))
    await user.click(await apply())
    expect(fix).toHaveBeenCalledExactlyOnceWith(['p2'], builtInLevels)
    finish({ changed: ['p2'], skipped: [] })
  })

  it('starts from the saved values, and changing them in the dialog saves nothing', async () => {
    const { file, saves, fix } = levelsDesktop()
    file.preferences['booharu-levels'] = { clip_percent: 2, whiten: true }
    const user = userEvent.setup()
    act(() => openLevels(['p1'], 'selected'))
    mount(null)

    const shown = await dialog()
    await apply()
    expect(within(shown).getByRole('switch', { name: 'Auto levels' })).toBeChecked()
    expect(within(shown).getByRole('switch', { name: 'Whiten paper' })).toBeChecked()
    expect(within(shown).getByRole('textbox', { name: 'Clip at each end (%)' })).toHaveValue('2')
    expect(within(shown).getByRole('textbox', { name: 'Whitening threshold (0–255)' })).toHaveValue(
      '235',
    )

    await user.click(within(shown).getByRole('switch', { name: 'Auto levels' }))
    const threshold = within(shown).getByRole('textbox', { name: 'Whitening threshold (0–255)' })
    await user.clear(threshold)
    await user.type(threshold, '200')
    await user.click(await apply())
    expect(fix).toHaveBeenCalledExactlyOnceWith(['p1'], {
      auto_levels: false,
      clip_percent: 2,
      whiten: true,
      threshold: 200,
    })
    expect(saves).toEqual([])
  })

  it('fixes nothing while Auto levels and Whiten paper are both off', async () => {
    levelsDesktop()
    const user = userEvent.setup()
    act(() => openLevels(['p1'], 'selected'))
    mount(null)

    const shown = await dialog()
    await apply()
    await user.click(within(shown).getByRole('switch', { name: 'Auto levels' }))
    expect(within(shown).getByRole('button', { name: 'Apply' })).toBeDisabled()
    expect(
      within(shown).getByText('Turn on Auto levels or Whiten paper to fix pages.'),
    ).toBeInTheDocument()
  })

  it('closes when another project opens, or its own closes', async () => {
    levelsDesktop()
    act(() => openLevels(['p1'], 'selected'))
    mount(null)
    await dialog()

    act(() => openProject('Another'))
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
    expect(useLevels.getState().selected).toBeNull()

    act(() => openLevels(['p1'], 'selected'))
    await dialog()
    act(() => queryClient.setQueryData(projectKey, null))
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  })
})

describe('Fix page levels: running it', () => {
  it('shows its progress, here and in the Activity center, and Stop ends it quietly', async () => {
    const { stop, progress, fail } = levelsDesktop()
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    act(() => openLevels(['p1', 'p2'], 'selected'))
    mount(<ActivityCenter />)

    const levels = await dialog()
    await user.click(await apply())
    expect(
      await within(levels).findByRole('button', { name: 'Fixing page levels…' }),
    ).toBeDisabled()
    expect(within(levels).getByRole('button', { name: 'Cancel' })).toBeDisabled()
    expect(within(levels).getByText('Fixing page levels…', { selector: 'p' })).toBeInTheDocument()
    // Behind the dialog, which hides the rest of the window from assistive technology while open.
    const activity = screen.getByRole('complementary', { name: 'Activity', hidden: true })
    expect(within(activity).getByText('Fixing page levels…')).toBeInTheDocument()

    progress({ done: 1, total: 2 })
    expect(within(levels).getByText('Fixing page levels: 1 of 2')).toBeInTheDocument()
    // Escape doesn't close it while it fixes.
    await user.keyboard('{Escape}')
    expect(screen.getByRole('dialog', { name: 'Fix page levels' })).toBeInTheDocument()

    await user.click(within(levels).getByRole('button', { name: 'Stop' }))
    await waitFor(() => expect(stop).toHaveBeenCalledOnce())
    fail('Fix page levels was stopped; no page changed.')
    expect(await within(levels).findByText('Stopped. No page changed.')).toBeInTheDocument()
    expect(within(levels).getByRole('button', { name: 'Apply' })).toBeEnabled()
    expect(shown).not.toHaveBeenCalled()
  })

  it('shows a refusal as every failed command does', async () => {
    const { fail } = levelsDesktop()
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    act(() => openLevels(['p1'], 'selected'))
    mount(null)

    await user.click(await apply())
    fail("Pages can't change while a run is going; wait for it to end, or stop it.")
    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'error',
          description: "Pages can't change while a run is going; wait for it to end, or stop it.",
        }),
      ),
    )
  })

  it('lists what it changed and each page it skipped, with why', async () => {
    const { finish } = levelsDesktop()
    const user = userEvent.setup()
    act(() => openLevels(pageIds, 'selected'))
    mount(null)

    const levels = await dialog()
    await user.click(await apply())
    finish({
      changed: ['p1'],
      skipped: [
        { page: 'p2', reason: 'cleanup' },
        { page: 'p3', reason: 'inpainted' },
      ],
    })
    const result = await within(levels).findByText('Fixed levels on 1 page.')
    expect(result.parentElement).toHaveTextContent(
      "Fixed levels on 1 page.Page 1Skipped 2 pages:Page 2: a box's clean-up is already on itPage 3: inpainting is already on it",
    )
    await user.click(within(levels).getByRole('button', { name: 'Done' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  })

  it('words every reason a page is skipped', () => {
    const reasons: Record<LevelsSkip, string> = {
      no_scan: 'no source image',
      cleanup: "a box's clean-up is already on it",
      inpainted: 'inpainting is already on it',
      unchanged: 'no change needed',
      scan_changed: 'its scan changed during the fix',
      no_original: 'no saved original',
      locked: 'its Original scan is in a locked layer or holds other layers',
    }
    for (const [reason, text] of Object.entries(reasons)) {
      expect(i18n.t(skipReason(reason as LevelsSkip)), reason).toBe(text)
    }
  })
})

describe('Fix page levels: the preview', () => {
  it('shows the first page before and after, through Blob URLs it revokes once replaced', async () => {
    const { preview } = levelsDesktop()
    let made = 0
    const create = vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:${++made}`)
    const revoke = vi.spyOn(URL, 'revokeObjectURL')
    const user = userEvent.setup()
    act(() => openLevels(['p2', 'p3'], 'selected'))
    mount(null)

    const levels = await dialog()
    expect(await within(levels).findByRole('img', { name: 'Before' })).toHaveAttribute(
      'src',
      'blob:1',
    )
    expect(within(levels).getByRole('img', { name: 'After' })).toHaveAttribute('src', 'blob:2')
    expect(within(levels).getByText('Preview: Page 2')).toBeInTheDocument()
    expect(within(levels).getByText('Black point: 12 · White point: 243')).toBeInTheDocument()
    expect(preview).toHaveBeenLastCalledWith('p2', builtInLevels)
    expect(create).toHaveBeenCalledTimes(2)
    expect(revoke).not.toHaveBeenCalled()

    const clip = within(levels).getByRole('textbox', { name: 'Clip at each end (%)' })
    await user.clear(clip)
    await user.type(clip, '3')
    await waitFor(() => expect(revoke).toHaveBeenCalledWith('blob:1'))
    expect(revoke).toHaveBeenCalledWith('blob:2')
    // The values settle, and the preview is read again for them.
    await waitFor(() =>
      expect(within(levels).getByRole('img', { name: 'Before' })).toHaveAttribute('src', 'blob:3'),
    )
    expect(preview).toHaveBeenLastCalledWith('p2', { ...builtInLevels, clip_percent: 3 })

    act(() => closeLevels())
    await waitFor(() => expect(revoke).toHaveBeenCalledWith('blob:4'))
    expect(revoke).toHaveBeenCalledWith('blob:3')
  })

  it('says it could not prepare a preview, with no toast', async () => {
    const { preview } = levelsDesktop()
    preview.mockRejectedValue('the page has no scan')
    const shown = vi.spyOn(toast, 'add')
    act(() => openLevels(['p1'], 'selected'))
    mount(null)

    const levels = await dialog()
    expect(await within(levels).findByText('Could not prepare this preview.')).toBeInTheDocument()
    expect(shown).not.toHaveBeenCalled()
  })
})

describe('Restore original scan', () => {
  it("shows in a page's menu only while its status says it has an Original scan", async () => {
    const { restore, status } = levelsDesktop(['p1'])
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    mount(<PageRail />)

    const plain = await pageMenu(user, 'Page 2')
    await within(plain).findByRole('menuitem', { name: 'Fix page levels…' })
    // Its status is read, and says it has none.
    await waitFor(() => expect(status).toHaveBeenCalledWith(['p2']))
    expect(within(plain).queryByRole('menuitem', { name: 'Restore original scan' })).toBeNull()
    await user.keyboard('{Escape}')

    const levelled = await pageMenu(user, 'Page 1')
    await user.click(
      await within(levelled).findByRole('menuitem', { name: 'Restore original scan' }),
    )
    // It asks nothing first.
    expect(screen.queryByRole('alertdialog')).toBeNull()
    await waitFor(() => expect(restore).toHaveBeenCalledExactlyOnceWith(['p1']))
    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'info', title: 'Restored 1 original page.' }),
      ),
    )
  })

  it('restores the selected pages that have one, and says why it skipped any', async () => {
    const { restore } = levelsDesktop(['p1', 'p3'])
    restore.mockResolvedValue({ changed: ['p1'], skipped: [{ page: 'p3', reason: 'locked' }] })
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    useKoharuStore.setState({ selectedPages: ['p1', 'p2', 'p3'] })
    mount(<PageRail />)

    const menu = await pageMenu(user, 'Page 2')
    await user.click(await within(menu).findByRole('menuitem', { name: 'Restore original scan' }))
    await waitFor(() => expect(restore).toHaveBeenCalledExactlyOnceWith(['p1', 'p3']))
    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith({
        type: 'info',
        title: 'Restored 1 original page.',
        description:
          'Skipped 1 page: Page 3: its Original scan is in a locked layer or holds other layers',
      }),
    )
  })
})

describe('Page levels in Settings > Pipeline', () => {
  it('saves each value to its own section at once', async () => {
    const { saves } = levelsDesktop()
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <PipelinePreferences value={{}} models={[]} onChange={() => undefined} />
      </QueryClientProvider>,
    )
    const heading = await screen.findByRole('heading', { name: 'Page levels', level: 3 })
    const section = within(heading.closest('section')!)
    const whiten = section.getByRole('switch', { name: 'Whiten paper' })
    await waitFor(() => expect(whiten).toBeEnabled())

    await user.click(whiten)
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]?.['booharu-levels']).toEqual({ ...builtInLevels, whiten: true })
    // Every other section as the file holds it.
    expect(saves[0]?.['booharu-lettering']).toEqual(builtInLettering)
    expect(saves[0]?.['booharu-reading']).toEqual(builtInReading)

    const threshold = section.getByRole('textbox', { name: 'Whitening threshold (0–255)' })
    await user.clear(threshold)
    await user.type(threshold, '210')
    await waitFor(() =>
      expect(completeLevels(saves.at(-1)?.['booharu-levels'])).toEqual({
        ...builtInLevels,
        whiten: true,
        threshold: 210,
      }),
    )
  })
})
