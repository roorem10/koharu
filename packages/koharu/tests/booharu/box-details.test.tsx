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
})
