import { QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { StartView } from '@/components/start/StartView'
import { saveBooharuSection } from '@/lib/booharu/preferences'
import { resources, supportedLanguages } from '@/lib/i18n'
import { queryClient } from '@/lib/queries'
import { commands } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { contextDesktop } from './context'
import { settingsDesktop } from './settings'

const title = 'Booharu 0.0.5 starts with fresh settings'
const folder = 'D:\\Booharu\\PreviewData\\settings\\series'

/** The home page, once its projects are listed. */
async function home() {
  render(
    <QueryClientProvider client={queryClient}>
      <StartView />
    </QueryClientProvider>,
  )
  await screen.findByText('No projects yet')
}

/** The desktop with no project yet, and the note's commands answering `folder`. */
function desktop(show = true, where: string | null = folder) {
  vi.spyOn(commands, 'listProjects').mockResolvedValue([])
  return {
    note: vi.spyOn(commands, 'booharuGetFirstStartNote').mockResolvedValue({ show, folder: where }),
    dismiss: vi.spyOn(commands, 'booharuDismissFirstStartNote').mockResolvedValue(null),
  }
}

describe('Booharu first-start note', () => {
  it('shows on the home page, saying where 0.0.4’s series are, until it is dismissed', async () => {
    const { note, dismiss } = desktop()
    const user = userEvent.setup()
    await home()

    const shown = within(await screen.findByRole('region', { name: title }))
    expect(
      shown.getByText(
        "Settings from Booharu 0.0.4 aren't carried over. To bring back its glossaries, use “Import series” in the full glossary.",
      ),
    ).toBeInTheDocument()
    expect(shown.getByText(`Booharu 0.0.4's series files are in ${folder}.`)).toBeInTheDocument()

    // It stays while the home page shows again, as nothing dismissed it.
    cleanup()
    await home()
    expect(await screen.findByRole('region', { name: title })).toBeInTheDocument()
    expect(dismiss).not.toHaveBeenCalled()

    await user.click(screen.getByRole('button', { name: 'Dismiss' }))
    expect(dismiss).toHaveBeenCalledOnce()
    await waitFor(() => expect(screen.queryByRole('region', { name: title })).toBeNull())

    // Next start, the desktop has it dismissed for good.
    note.mockResolvedValue({ show: false, folder })
    cleanup()
    queryClient.clear()
    await home()
    await waitFor(() => expect(note).toHaveBeenCalledTimes(2))
    expect(screen.queryByRole('region', { name: title })).toBeNull()
  })

  it('names where 0.0.4 keeps its series when they are not beside the program', async () => {
    desktop(true, null)
    await home()

    const shown = within(await screen.findByRole('region', { name: title }))
    expect(
      shown.getByText(
        'Booharu 0.0.4 keeps its series files in PreviewData\\settings\\series, next to its Booharu.exe.',
      ),
    ).toBeInTheDocument()
  })

  it('keeps the note when dismissing it fails, and shows why', async () => {
    const { dismiss } = desktop()
    dismiss.mockRejectedValue('could not write koharu.toml')
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    await home()

    await user.click(
      within(await screen.findByRole('region', { name: title })).getByRole('button', {
        name: 'Dismiss',
      }),
    )
    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'error', description: 'could not write koharu.toml' }),
      ),
    )
    expect(screen.getByRole('region', { name: title })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Dismiss' })).toBeEnabled()
  })

  it('dismisses in turn with a settings save, so the save never puts the note back', async () => {
    const { file } = settingsDesktop()
    const { dismiss } = desktop()
    // A section save under way: it read every section before the dismiss, and saves them all.
    let release = () => {}
    vi.spyOn(commands, 'booharuSavePreferences').mockImplementation(
      (preferences) =>
        new Promise((resolve) => {
          release = () => {
            file.preferences = structuredClone(preferences)
            resolve(structuredClone(preferences))
          }
        }),
    )
    const saving = saveBooharuSection('booharu-runs', (section) => section)
    const user = userEvent.setup()
    await home()

    await user.click(
      within(await screen.findByRole('region', { name: title })).getByRole('button', {
        name: 'Dismiss',
      }),
    )
    await waitFor(() => expect(commands.booharuSavePreferences).toHaveBeenCalledOnce())
    expect(dismiss).not.toHaveBeenCalled()

    release()
    await saving
    await waitFor(() => expect(dismiss).toHaveBeenCalledOnce())
    await waitFor(() => expect(screen.queryByRole('region', { name: title })).toBeNull())
  })

  it('opens the glossary editor, where Import series is', async () => {
    contextDesktop()
    desktop()
    const user = userEvent.setup()
    await home()

    await user.click(
      within(await screen.findByRole('region', { name: title })).getByRole('button', {
        name: 'Open full glossary',
      }),
    )
    const editor = within(await screen.findByRole('dialog', { name: 'Glossary' }))
    expect(editor.getByRole('button', { name: 'Import series' })).toBeInTheDocument()
    expect(
      editor.getByText('Pick or create a series to keep the glossary across chapters.'),
    ).toBeInTheDocument()
  })

  it('shows nothing when the desktop cannot say whether to', async () => {
    desktop()
    vi.spyOn(commands, 'booharuGetFirstStartNote').mockRejectedValue('no desktop')
    await home()

    expect(screen.queryByRole('region', { name: title })).toBeNull()
  })

  it('has its strings in every language', () => {
    const start = (language: string) =>
      Object.keys(
        (resources[language].translation as { booharu: { start: object } }).booharu.start,
      ).sort()
    const english = start('en-US')
    expect(english).toEqual(['body', 'dismiss', 'folder', 'folderUnknown', 'title'])
    for (const language of supportedLanguages) expect(start(language), language).toEqual(english)
  })
})
