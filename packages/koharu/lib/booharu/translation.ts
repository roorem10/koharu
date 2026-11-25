'use client'

import { typedByHand } from '@/lib/booharu/attrs'
import { plainText, type BlockLayer } from '@/lib/document'
import { send } from '@/lib/queries'
import { setText } from '@/lib/scene'
import { commands, type Change, type LayerId } from '@koharu/ipc/protocol'

/** Booharu: whether `block`'s translation is a placeholder: the blank a block gets when it's styled
 * before it's translated, as from the type bar, holding that style (P-d), which the scene starts as
 * its source's blank, in its source's place and frame (Koharu's #1494), so its frame tells nothing.
 * The block is still untranslated: typing into it writes its first translation, and a reader that
 * asks whether a box is translated reads it as not. Blank is as the desktop's `trim` reads it,
 * Unicode's White_Space. A translation typed by hand starts as the same blank, so one cleared
 * since, or typed blank, carries the marker of one typed by hand in the block's record
 * (`typedByHand`) and is no placeholder: a hand edit, left blank on purpose, retyped in its own
 * place. No translation is not a placeholder, so ask that apart. The desktop's own rule also leaves
 * out a blank translation a run wrote, which only the record's hash tells apart, so
 * `commitTranslation` leaves that to the desktop. */
export function isPlaceholder(block: BlockLayer): boolean {
  const { translation } = block
  return (
    translation != null && /^\p{White_Space}*$/u.test(plainText(translation)) && !typedByHand(block)
  )
}

/** Booharu: a block's first translation, typed in the Story panel. The desktop command sets the
 * text where the scene starts a first translation, in its source's place (Koharu's #1494), a
 * placeholder there in the style set on it, and gives it what a typed translation lacks in Koharu:
 * the default look, the target language, the block's clean-up shown and the marker of a
 * translation typed by hand, all in one undo step. A translation of the block's own, a run's
 * emptied by hand or one typed by hand and cleared since among them, it retypes as `set_text`
 * does, the step labelled as Story labels one. */
export function setTranslation(block: LayerId, text: string): Promise<Change> {
  return commands.booharuSetTranslation(block, text)
}

/** Commits `text` as `block`'s translation, as one undo step, as typing it in Story does: in place
 * of the text of the translation it has, or as its first, which `setTranslation` writes. A
 * placeholder isn't a translation here, so typing into a block styled before it was translated
 * writes its first translation too, in that style (P-d), and any other blank one with no marker of
 * a translation typed by hand goes to the desktop, which tells a placeholder from a run's emptied
 * by hand; one typed by hand and cleared is retyped in its own place. Story's field and Explain
 * this line's Use this both commit through it. A failure shows its error, as every failed
 * command's does. */
export function commitTranslation(block: BlockLayer, text: string): void {
  const first = !block.translation || isPlaceholder(block)
  send(first ? setTranslation : setText, block.id, text)
}
