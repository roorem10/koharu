import { QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { BoxNoteLine } from '@/components/editor/BoxNoteLine'
import { projectKey, queryClient } from '@/lib/queries'
import { commands } from '@koharu/bridge/protocol'

function editor(layer = 'line') {
  queryClient.setQueryData(projectKey, {
    name: 'notes',
    revision: 1,
    active_page: null,
    can_undo: false,
    can_redo: false,
  })
  return (
    <QueryClientProvider client={queryClient}>
      <BoxNoteLine layer={layer} />
    </QueryClientProvider>
  )
}

describe('Inspector box notes', () => {
  it('shows a saved note, saves on blur, and clears whitespace', async () => {
    vi.mocked(commands.getBoxNote).mockResolvedValue('Check the pun')
    const save = vi.mocked(commands.setBoxNote)
    render(editor())
    const note = await screen.findByRole('textbox', { name: 'Box note' })
    expect(note).toHaveValue('Check the pun')
    const user = userEvent.setup()
    await user.clear(note)
    await user.type(note, 'Reword the joke')
    expect(save).not.toHaveBeenCalled()
    await user.tab()
    await waitFor(() => expect(save).toHaveBeenLastCalledWith('line', 'Reword the joke'))
    await waitFor(() => expect(note).toBeEnabled())
    await user.clear(note)
    await user.type(note, '   ')
    await user.tab()
    await waitFor(() => expect(save).toHaveBeenLastCalledWith('line', null))
  })

  it('keeps edits visible after a save fails and lets the user retry', async () => {
    vi.mocked(commands.getBoxNote).mockResolvedValue('Original')
    const save = vi.mocked(commands.setBoxNote).mockRejectedValueOnce(new Error('Disk full'))
    render(editor())
    const note = await screen.findByRole('textbox', { name: 'Box note' })
    fireEvent.change(note, { target: { value: 'Keep this edit' } })
    fireEvent.blur(note)
    await screen.findByRole('alert')
    expect(note).toHaveValue('Keep this edit')
    fireEvent.blur(note)
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument())
  })

  it('does not display a previous box note when selection changes', async () => {
    vi.mocked(commands.getBoxNote).mockImplementation(async (layer) =>
      layer === 'first' ? 'First note' : null,
    )
    const view = render(editor('first'))
    expect(await screen.findByRole('textbox', { name: 'Box note' })).toHaveValue('First note')
    view.rerender(editor('second'))
    expect(screen.queryByDisplayValue('First note')).not.toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Note' }))
    expect(await screen.findByRole('textbox', { name: 'Box note' })).toHaveValue('')
  })
})
