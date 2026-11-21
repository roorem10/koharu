'use client'

import { create } from 'zustand'

import { call } from '@/lib/booharu/call'
import i18n from '@/lib/i18n'
import { commands, type LayerId, type RedoAnswer, type RedoCounts } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

/** A stage Redo boxes asks about before it replaces what was fixed by hand: Read (`recognize`), a
 * source the user corrected, and Translate, a translation the user wrote or fixed. Clean up asks
 * nothing, so its box action runs the pipeline at once. */
export type RedoStage = 'recognize' | 'translate'

/** An answer to the confirmation: run on the boxes fixed by hand too (`overwrite`), run on the
 * others alone (`skip`), or run nothing (`cancel`). */
export type RedoChoice = 'overwrite' | 'skip' | 'cancel'

/** A redo waiting on the confirmation: the boxes and the stage it runs, and how they stand. */
export interface PendingRedo {
  blocks: LayerId[]
  stage: RedoStage
  counts: RedoCounts
}

interface RedoState {
  /** The redo the confirmation asks about; null while it's closed. */
  pending: PendingRedo | null
}

/** The redo confirmation's state, the interface's own, apart from the project and Koharu's store. */
export const useRedo = create<RedoState>()(() => ({ pending: null }))

/** Gives the open confirmation's answer to the redo waiting on it. */
let answer: ((choice: RedoChoice) => void) | null = null

/** Opens the confirmation on `pending` and waits for its answer. A redo still waiting on one is
 * cancelled, as one confirmation asks at a time. */
function ask(pending: PendingRedo): Promise<RedoChoice> {
  answer?.('cancel')
  return new Promise((resolve) => {
    answer = (choice) => {
      answer = null
      useRedo.setState({ pending: null })
      resolve(choice)
    }
    useRedo.setState({ pending })
  })
}

/** Answers the open confirmation, which closes; nothing while none is open. */
export function answerRedo(choice: RedoChoice): void {
  answer?.(choice)
}

/** Runs `stage` again on the boxes `blocks`, as Redo boxes does: it counts them first, and when any
 * was fixed by hand for the stage, the confirmation asks whether to overwrite them, run on the
 * others, or run nothing; otherwise it runs at once. A toast then says what the run left alone, as
 * fixed by hand or locked. Answers what Redo boxes did, or null when the user ran nothing. A failed
 * command shows its error, as every command's does, and rejects. */
export async function redoBoxes(blocks: LayerId[], stage: RedoStage): Promise<RedoAnswer | null> {
  const counts = await call(commands.booharuCountRedo, blocks, [stage])
  const choice = counts.hand_fixed > 0 ? await ask({ blocks, stage, counts }) : 'skip'
  if (choice === 'cancel') return null
  const done = await call(commands.booharuRedoBoxes, blocks, [stage], choice === 'overwrite')
  redoNotice(done)
  return done
}

/** The toast after a redo, `done`, when it left boxes alone, as fixed by hand or locked, or ran on
 * none; none when it ran on every box. */
export function redoNotice(done: RedoAnswer): void {
  const lines = [
    done.hand_fixed > 0 ? i18n.t('booharu.redo.keptHandFixed', { count: done.hand_fixed }) : null,
    done.locked > 0 ? i18n.t('booharu.redo.keptLocked', { count: done.locked }) : null,
  ].filter((line): line is string => line !== null)
  if (done.run === null) {
    toast.add({
      type: 'info',
      title: i18n.t('booharu.redo.nothing'),
      description: lines.join(' ') || undefined,
    })
  } else if (lines.length > 0) {
    toast.add({ type: 'info', title: lines[0], description: lines[1] })
  }
}
