import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it } from 'vitest'

import { TitleBar } from '@/components/app/TitleBar'
import { RedoDialog } from '@/components/booharu/boxes/RedoDialog'
import { StoryPanel } from '@/components/editor/StoryPanel'
import { useBoxDetails } from '@/lib/booharu/boxDetails'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'

import { boxesPage, desktop, openProject } from './boxes'

function show(ui: ReactNode) {
  render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>)
}

/** The items of the menu open now, by name, each with whether it's enabled; a menubar's own
 * triggers are menu items too, so only those in an open popup count. */
async function items(): Promise<[string, boolean][]> {
  const open = () =>
    screen.queryAllByRole('menuitem').filter((item) => item.closest('[data-slot$="-content"]'))
  await waitFor(() => expect(open()).not.toHaveLength(0))
  return open().map((item) => [
    item.textContent ?? '',
    item.getAttribute('aria-disabled') !== 'true',
  ])
}

/** Opens the "…" menu of the Story row named `name`. */
async function openRow(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(screen.getByRole('button', { name: `Actions for ${name}` }))
}

/** Opens the Edit menu. */
async function openEdit(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('menuitem', { name: 'Edit' }))
}

/** Booharu's items in the Edit menu, in order. */
const boxItems = [
  'Read again',
  'Translate again',
  'Clean again',
  'Join with balloon',
  'Split',
  'Add subtitle',
  'Change category…',
  'Reset to lettering default',
  'Explain this line',
  'Box details',
  'Join split texts on this page',
]

beforeEach(() => useBoxDetails.setState({ shown: null, reveal: null }))

describe('Booharu box actions in the menus', () => {
  it("lists a Story row's box actions above Delete and runs them on its text", async () => {
    const { redo, join } = desktop()
    openProject(boxesPage())
    const user = userEvent.setup()
    show(<StoryPanel />)

    await openRow(user, 'lo')
    expect(await items()).toEqual([
      ['Read again', true],
      ['Translate again', true],
      ['Clean again', true],
      ['Join with balloon', true],
      ['Split', true],
      ['Add subtitle', false],
      ['Change category…', true],
      ['Reset to lettering default', false],
      ['Explain this line', true],
      ['Box details', true],
      ['Delete', true],
    ])
    fireEvent.click(screen.getByRole('menuitem', { name: 'Translate again' }))
    await waitFor(() => expect(redo).toHaveBeenCalledWith(['second'], ['translate'], false))

    await openRow(user, 'lo')
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Join with balloon' }))
    await waitFor(() => expect(join).toHaveBeenCalledWith('second'))

    // A text layer has nothing to read, translate, clean, join, split or explain, and no category,
    // though its look can be reset; a locked block takes no change, though its line can be
    // explained.
    await openRow(user, 'Caption')
    expect((await items()).filter(([, on]) => on).map(([name]) => name)).toEqual([
      'Reset to lettering default',
      'Box details',
      'Delete',
    ])
    await user.keyboard('{Escape}')
    await openRow(user, 'Locked')
    expect((await items()).filter(([, on]) => on).map(([name]) => name)).toEqual([
      'Explain this line',
      'Box details',
    ])
  })

  it("offers Add subtitle on a sound effect once the page's lettering is read", async () => {
    const { lettering, subtitle } = desktop()
    lettering.mockResolvedValue([
      {
        block: 'bang',
        category: 'sfx',
        reason: 'sfx',
        label: null,
        shape: null,
        hand_edited: false,
      },
    ])
    openProject(boxesPage())
    const user = userEvent.setup()
    show(<StoryPanel />)

    await openRow(user, 'Bang')
    const item = await screen.findByRole('menuitem', { name: 'Add subtitle' })
    await waitFor(() => expect(item).not.toHaveAttribute('aria-disabled', 'true'))
    expect(lettering).toHaveBeenCalledWith('page')
    fireEvent.click(item)
    await waitFor(() => expect(subtitle).toHaveBeenCalledWith('bang', null))
    // The subtitle is selected, as the canvas shows its page.
    await waitFor(() => expect(useKoharuStore.getState().selectedLayers).toEqual(['bang-subtitle']))

    // A text the lettering doesn't call a sound effect takes none.
    await openRow(user, 'lo')
    expect(await screen.findByRole('menuitem', { name: 'Add subtitle' })).toHaveAttribute(
      'aria-disabled',
      'true',
    )
  })

  it("sends a Story row's Translate through Redo boxes, which asks before it overwrites", async () => {
    const { run, countRedo, redo } = desktop()
    countRedo.mockResolvedValue({ ready: 0, hand_fixed: 1, locked: 0 })
    openProject(boxesPage())
    const user = userEvent.setup()
    show(
      <>
        <StoryPanel />
        <RedoDialog />
      </>,
    )

    await user.click(screen.getByRole('button', { name: 'Translate lo' }))
    expect(
      await screen.findByRole('alertdialog', { name: 'Overwrite hand-edited text?' }),
    ).toHaveAccessibleDescription(
      '1 of the selected boxes has a translation you wrote or fixed by hand. Translate it again and replace it?',
    )
    expect(countRedo).toHaveBeenCalledWith(['second'], ['translate'])
    expect(redo).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: 'Overwrite and run' }))
    await waitFor(() => expect(redo).toHaveBeenCalledWith(['second'], ['translate'], true))
    expect(run).not.toHaveBeenCalled()
  })

  it('lists the box actions for the selection in the Edit menu', async () => {
    const { run, redo, split } = desktop()
    openProject(boxesPage())
    // A block's clean-up stands for the block.
    useKoharuStore.getState().selectLayers(['first-cleanup'])
    const user = userEvent.setup()
    show(<TitleBar />)

    await openEdit(user)
    // Between Koharu's Delete Selected, whose name ends with its shortcut, and Unload Models.
    const names = (await items()).map(([name]) => name)
    const after = names.findIndex((name) => name.startsWith('Delete Selected'))
    expect(names.slice(after + 1, names.indexOf('Unload Models'))).toEqual(boxItems)
    fireEvent.click(screen.getByRole('menuitem', { name: 'Clean again' }))
    await waitFor(() => expect(run).toHaveBeenCalledWith({ blocks: ['first'] }, ['inpaint']))

    await openEdit(user)
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Split' }))
    await waitFor(() => expect(split).toHaveBeenCalledWith('first'))

    // Three texts: the runs, over the blocks, and Reset, which the text layer takes.
    useKoharuStore.getState().selectLayers(['free', 'caption', 'alone'])
    await openEdit(user)
    expect(
      (await items()).filter(([name, on]) => on && boxItems.includes(name)).map(([name]) => name),
    ).toEqual([
      'Read again',
      'Translate again',
      'Clean again',
      'Reset to lettering default',
      'Join split texts on this page',
    ])
    fireEvent.click(screen.getByRole('menuitem', { name: 'Read again' }))
    await waitFor(() =>
      expect(redo).toHaveBeenLastCalledWith(['free', 'alone'], ['recognize'], false),
    )
  })

  it("joins the page's split texts while the desktop finds some there", async () => {
    const { joinPage, findSplit } = desktop()
    openProject(boxesPage())
    useKoharuStore.getState().selectLayers(['second'])
    const user = userEvent.setup()
    show(<TitleBar />)

    await openEdit(user)
    const item = await screen.findByRole('menuitem', { name: 'Join split texts on this page' })
    await waitFor(() => expect(item).not.toHaveAttribute('aria-disabled', 'true'))
    expect(findSplit).toHaveBeenCalledWith('page')
    fireEvent.click(item)
    await waitFor(() => expect(joinPage).toHaveBeenCalledWith('page'))
    // The joined text that went leaves the selection to the one it joined into.
    await waitFor(() => expect(useKoharuStore.getState().selectedLayers).toEqual(['first']))

    // Once none is left, there's nothing to join.
    findSplit.mockResolvedValue([])
    queryClient.removeQueries({ queryKey: ['page', 'booharu'] })
    await openEdit(user)
    expect(
      await screen.findByRole('menuitem', { name: 'Join split texts on this page' }),
    ).toHaveAttribute('aria-disabled', 'true')
  })

  it('disables the page join while a lock in any split balloon would refuse it all', async () => {
    const { findSplit } = desktop()
    // The desktop finds the held balloon's text split too.
    findSplit.mockResolvedValue([
      { page: 'page', balloon: 'split', blocks: ['first', 'second'] },
      { page: 'page', balloon: 'held', blocks: ['held-first', 'held-second'] },
    ])
    const unlocked = boxesPage()
    unlocked.layers[0]!.children![2]!.locked = false
    openProject(unlocked)
    const user = userEvent.setup()
    show(<TitleBar />)

    await openEdit(user)
    const item = await screen.findByRole('menuitem', { name: 'Join split texts on this page' })
    await waitFor(() => expect(item).not.toHaveAttribute('aria-disabled', 'true'))
    // Locked, the held balloon would refuse the join of both.
    act(() => openProject(boxesPage()))
    await waitFor(() => expect(item).toHaveAttribute('aria-disabled', 'true'))
  })
})
