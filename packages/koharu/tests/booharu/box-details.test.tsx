import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { TitleBar } from '@/components/app/TitleBar'
import { BoxDetailsDialog } from '@/components/booharu/details/BoxDetailsDialog'
import { StoryPanel } from '@/components/editor/StoryPanel'
import { openBoxDetails, useBoxDetails } from '@/lib/booharu/boxDetails'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import type { Page } from '@koharu/ipc/protocol'

import { boxesPage, desktop, openProject } from './boxes'

function mount(ui: ReactNode) {
  render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>)
}

/** Box details, once it shows. */
function details(): Promise<HTMLElement> {
  return screen.findByRole('dialog', { name: 'Box details' })
}

async function closed() {
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
}

/** The boxes page with a reviewer's note on "lo" and one on the locked block. */
function notedPage(): Page {
  const page = boxesPage()
  page.layers[0]!.children![0]!.children![1]!.attributes = {
    booharu: { review_note: 'Check the tone' },
  }
  page.layers[2]!.attributes = { booharu: { review_note: 'Leave as is' } }
  return page
}

const lockedReason =
  "This text, or a layer it's in, is locked. Unlock it in Layers to change its note."

beforeEach(() => useBoxDetails.setState({ shown: null, reveal: null }))

describe('Booharu Box details', () => {
  it('opens on the text of the ? tag, the Story row menu or the Edit menu', async () => {
    const { split } = desktop()
    openProject(boxesPage())
    const user = userEvent.setup()
    mount(
      <>
        <TitleBar />
        <StoryPanel />
        <BoxDetailsDialog />
      </>,
    )

    await user.click(screen.getByRole('button', { name: 'Box details for lo' }))
    let dialog = await details()
    expect(useBoxDetails.getState().shown).toEqual({ page: 'page', layer: 'second' })
    expect(within(dialog).getByText('lo')).toBeInTheDocument()
    // Its actions are the text's box actions, Box details itself left out.
    const actions = within(within(dialog).getByRole('region', { name: 'Actions' }))
    expect(actions.getAllByRole('button').map((button) => button.textContent)).toEqual([
      'Read again',
      'Translate again',
      'Clean again',
      'Join with balloon',
      'Split',
    ])
    await user.click(actions.getByRole('button', { name: 'Split' }))
    await waitFor(() => expect(split).toHaveBeenCalledWith('second'))
    await user.click(within(dialog).getByRole('button', { name: 'Close' }))
    await closed()

    await user.click(screen.getByRole('button', { name: 'Actions for Caption' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Box details' }))
    dialog = await details()
    expect(useBoxDetails.getState().shown).toEqual({ page: 'page', layer: 'caption' })
    expect(within(dialog).getByText('Caption')).toBeInTheDocument()
    await user.keyboard('{Escape}')
    await closed()

    // A block's mask stands for the block.
    act(() => useKoharuStore.getState().selectLayers(['first-mask']))
    await user.click(screen.getByRole('menuitem', { name: 'Edit' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Box details' }))
    dialog = await details()
    expect(useBoxDetails.getState().shown).toEqual({ page: 'page', layer: 'first' })
    expect(within(dialog).getByText('Hel')).toBeInTheDocument()
  })

  it('shows its text in Story: the Story tab, the text selected and its row in view', async () => {
    desktop()
    openProject(boxesPage())
    useKoharuStore.getState().selectLayers(['second'])
    const user = userEvent.setup()
    mount(
      <>
        <StoryPanel />
        <BoxDetailsDialog />
      </>,
    )

    act(() => openBoxDetails('page', 'first'))
    await user.click(within(await details()).getByRole('button', { name: 'Show in Story' }))
    await closed()
    expect(useKoharuStore.getState().panel).toBe('story')
    expect(useKoharuStore.getState().selectedLayers).toEqual(['first'])

    // A text already selected stays so, and its row is brought into view all the same.
    const scroll = vi.mocked(Element.prototype.scrollIntoView)
    const row = screen
      .getByRole('button', { name: 'Box details for Hel' })
      .closest('[data-selected]')
    act(() => openBoxDetails('page', 'first'))
    const button = within(await details()).getByRole('button', { name: 'Show in Story' })
    scroll.mockClear()
    await user.click(button)
    await waitFor(() => expect(scroll.mock.contexts).toContain(row))
  })

  it('shows the page of a text the canvas does not show, then selects the text', async () => {
    const { show } = desktop()
    openProject(boxesPage(), null)
    const user = userEvent.setup()
    mount(<BoxDetailsDialog />)

    act(() => openBoxDetails('page', 'caption'))
    await user.click(within(await details()).getByRole('button', { name: 'Show in Story' }))
    await waitFor(() => expect(useKoharuStore.getState().selectedLayers).toEqual(['caption']))
    expect(show).toHaveBeenCalledWith('page')
    expect(useKoharuStore.getState().selectedPages).toEqual(['page'])
    expect(useKoharuStore.getState().panel).toBe('story')
  })

  it('saves the note when its field loses focus, and removes it when left blank', async () => {
    const { note } = desktop()
    openProject(notedPage())
    const user = userEvent.setup()
    mount(<BoxDetailsDialog />)

    act(() => openBoxDetails('page', 'first'))
    const field = within(await details()).getByRole('textbox', { name: 'Box note' })
    expect(field).toHaveValue('')
    // Visiting the field without changing it saves nothing.
    await user.click(field)
    await user.tab()
    await user.type(field, 'Too long for the balloon')
    expect(note).not.toHaveBeenCalled()
    await user.tab()
    expect(note).toHaveBeenCalledOnce()
    expect(note).toHaveBeenCalledWith('first', 'Too long for the balloon')

    act(() => openBoxDetails('page', 'second'))
    const noted = within(await details()).getByRole('textbox', { name: 'Box note' })
    expect(noted).toHaveValue('Check the tone')
    await user.clear(noted)
    await user.type(noted, '  ')
    await user.tab()
    expect(note).toHaveBeenCalledTimes(2)
    expect(note).toHaveBeenLastCalledWith('second', null)
  })

  it('saves the note being typed when Escape closes Box details, and only once', async () => {
    const { note } = desktop()
    openProject(notedPage())
    const user = userEvent.setup()
    mount(<BoxDetailsDialog />)

    act(() => openBoxDetails('page', 'first'))
    let field = within(await details()).getByRole('textbox', { name: 'Box note' })
    await user.click(field)
    await user.type(field, 'Check the tone')
    await user.keyboard('{Escape}')
    await closed()
    expect(note.mock.calls).toEqual([['first', 'Check the tone']])

    // Saved as the field lost focus, it isn't saved again as Box details closes.
    act(() => openBoxDetails('page', 'second'))
    field = within(await details()).getByRole('textbox', { name: 'Box note' })
    await user.click(field)
    await user.type(field, '!')
    await user.click(within(await details()).getByRole('button', { name: 'Close' }))
    await closed()
    expect(note.mock.calls).toEqual([
      ['first', 'Check the tone'],
      ['second', 'Check the tone!'],
    ])
  })

  it('says why a locked text takes no note, and saves none', async () => {
    const { note } = desktop()
    openProject(notedPage())
    mount(<BoxDetailsDialog />)

    // A locked block shows the note it has, read-only.
    act(() => openBoxDetails('page', 'locked'))
    let dialog = await details()
    expect(within(dialog).getByText('Leave as is')).toBeInTheDocument()
    expect(within(dialog).getByText(lockedReason)).toBeInTheDocument()
    expect(within(dialog).queryByRole('textbox')).toBeNull()

    // A block in a locked balloon is locked with it.
    act(() => openBoxDetails('page', 'held-first'))
    dialog = await details()
    expect(within(dialog).getByText(lockedReason)).toBeInTheDocument()
    expect(within(dialog).queryByRole('textbox')).toBeNull()
    expect(note).not.toHaveBeenCalled()
  })
})
