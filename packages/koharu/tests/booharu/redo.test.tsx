import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { RedoDialog } from '@/components/booharu/boxes/RedoDialog'
import { answerRedo, redoBoxes, redoNotice, useRedo } from '@/lib/booharu/redo'
import { commands, type RedoAnswer, type RedoCounts } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

const TITLE = 'Overwrite hand-edited text?'

/** The desktop counting the boxes a redo names as `counts`, and answering the redo with `done`. */
function redoDesktop(counts: RedoCounts, done: RedoAnswer = { run: 7, hand_fixed: 0, locked: 0 }) {
  const count = vi.spyOn(commands, 'booharuCountRedo').mockResolvedValue(counts)
  const redo = vi.spyOn(commands, 'booharuRedoBoxes').mockResolvedValue(done)
  return { count, redo }
}

/** The confirmation, once it asks. */
function confirmation() {
  return screen.findByRole('alertdialog', { name: TITLE })
}

// A confirmation an earlier test left open runs nothing.
beforeEach(() => answerRedo('cancel'))

describe('Booharu Redo boxes', () => {
  it('runs at once, asking nothing, when no box was fixed by hand', async () => {
    const { count, redo } = redoDesktop({ ready: 2, hand_fixed: 0, locked: 1 })
    const shown = vi.spyOn(toast, 'add')
    render(<RedoDialog />)

    const done = { run: 7, hand_fixed: 0, locked: 1 }
    redo.mockResolvedValue(done)
    await expect(redoBoxes(['first', 'second', 'locked'], 'translate')).resolves.toEqual(done)
    expect(count).toHaveBeenCalledWith(['first', 'second', 'locked'], ['translate'])
    expect(redo).toHaveBeenCalledWith(['first', 'second', 'locked'], ['translate'], false)
    expect(screen.queryByRole('alertdialog')).toBeNull()
    expect(useRedo.getState().pending).toBeNull()
    // The locked box the run left alone.
    expect(shown).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'info', title: '1 locked box was left as it is.' }),
    )
  })

  it('asks before overwriting, and Overwrite and run takes the boxes fixed by hand too', async () => {
    const { redo } = redoDesktop({ ready: 1, hand_fixed: 2, locked: 0 })
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    render(<RedoDialog />)

    const redone = redoBoxes(['first', 'second', 'third'], 'recognize')
    const dialog = await confirmation()
    expect(dialog).toHaveAccessibleDescription(
      '2 of the selected boxes have a source you corrected by hand. Read them again and replace it? Translations are kept.',
    )
    expect(redo).not.toHaveBeenCalled()

    await user.click(screen.getByRole('button', { name: 'Overwrite and run' }))
    await expect(redone).resolves.toEqual({ run: 7, hand_fixed: 0, locked: 0 })
    expect(redo).toHaveBeenCalledTimes(1)
    expect(redo).toHaveBeenCalledWith(['first', 'second', 'third'], ['recognize'], true)
    await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull())
    // It ran on every box: nothing to say.
    expect(shown).not.toHaveBeenCalled()
  })

  it('Skip those runs on the others, and says how many it left alone', async () => {
    const { redo } = redoDesktop(
      { ready: 2, hand_fixed: 1, locked: 0 },
      { run: 8, hand_fixed: 1, locked: 0 },
    )
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    render(<RedoDialog />)

    const redone = redoBoxes(['first', 'second', 'fixed'], 'translate')
    expect(await confirmation()).toHaveAccessibleDescription(
      '1 of the selected boxes has a translation you wrote or fixed by hand. Translate it again and replace it?',
    )
    await user.click(screen.getByRole('button', { name: 'Skip those' }))
    await expect(redone).resolves.toEqual({ run: 8, hand_fixed: 1, locked: 0 })
    expect(redo).toHaveBeenCalledTimes(1)
    expect(redo).toHaveBeenCalledWith(['first', 'second', 'fixed'], ['translate'], false)
    expect(shown).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'info', title: '1 box fixed by hand was left as it is.' }),
    )
  })

  it('Cancel, or Escape, runs nothing', async () => {
    const { redo } = redoDesktop({ ready: 3, hand_fixed: 2, locked: 0 })
    const user = userEvent.setup()
    render(<RedoDialog />)

    const cancelled = redoBoxes(['first', 'second'], 'translate')
    await confirmation()
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    await expect(cancelled).resolves.toBeNull()
    await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull())

    const escaped = redoBoxes(['first', 'second'], 'recognize')
    await confirmation()
    await user.keyboard('{Escape}')
    await expect(escaped).resolves.toBeNull()
    expect(redo).not.toHaveBeenCalled()
  })

  it('offers no Skip those when every box was fixed by hand, as it would run on none', async () => {
    const { redo } = redoDesktop({ ready: 0, hand_fixed: 2, locked: 1 })
    const user = userEvent.setup()
    render(<RedoDialog />)

    const redone = redoBoxes(['first', 'second', 'locked'], 'translate')
    await confirmation()
    expect(screen.queryByRole('button', { name: 'Skip those' })).toBeNull()
    await user.click(screen.getByRole('button', { name: 'Overwrite and run' }))
    await redone
    expect(redo).toHaveBeenCalledWith(['first', 'second', 'locked'], ['translate'], true)
  })

  it('a second redo cancels the one still waiting on the confirmation', async () => {
    const { redo } = redoDesktop({ ready: 1, hand_fixed: 1, locked: 0 })
    const user = userEvent.setup()
    render(<RedoDialog />)

    const first = redoBoxes(['first'], 'recognize')
    await confirmation()
    const second = redoBoxes(['second'], 'translate')
    await expect(first).resolves.toBeNull()
    expect(await confirmation()).toHaveAccessibleDescription(
      '1 of the selected boxes has a translation you wrote or fixed by hand. Translate it again and replace it?',
    )
    await user.click(screen.getByRole('button', { name: 'Overwrite and run' }))
    await second
    expect(redo).toHaveBeenCalledTimes(1)
    expect(redo).toHaveBeenCalledWith(['second'], ['translate'], true)
  })

  it('says when a redo ran on nothing, and why', () => {
    const shown = vi.spyOn(toast, 'add')
    redoNotice({ run: null, hand_fixed: 2, locked: 3 })
    expect(shown).toHaveBeenCalledWith({
      type: 'info',
      title: 'Nothing to run',
      description:
        '2 boxes fixed by hand were left as they are. 3 locked boxes were left as they are.',
    })
  })
})
