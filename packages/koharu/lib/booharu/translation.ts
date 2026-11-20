'use client'

import { dispatch } from '@/lib/backend'
import { plainText, type BlockLayer } from '@/lib/document'
import { setText } from '@/lib/scene'
import { commands, type Change, type LayerId, type Text } from '@koharu/ipc/protocol'

/** Booharu: whether a block's `translation` is a placeholder: the blank point text a block gets
 * when it's styled before it's translated, as from the type bar, holding that style (P-d). The block
 * is still untranslated: typing into it places the text in its frame, and a reader that asks whether
 * a box is translated reads it as not. Blank is as the desktop's `trim` reads it, Unicode's
 * White_Space, and a translation with no frame is point text, as the desktop reads one. No
 * translation is not a placeholder, so ask that apart. The desktop's own rule also leaves out blank
 * point text a run wrote. */
export function isPlaceholder(translation: Text | null | undefined): boolean {
  return (
    translation != null &&
    (translation.frame ?? 'point') === 'point' &&
    /^\p{White_Space}*$/u.test(plainText(translation))
  )
}

/** Booharu: a block's first translation, typed in the Story panel (#128). The scene's `set_text`
 * alone gives a block without a translation, or with only a placeholder, point text at the page's
 * top-left corner, so the desktop command sets the text and writes the translation as the
 * Translation stage writes one it places: in the block's frame, in the target language, with the
 * block's clean-up shown, all in one undo step. */
export function setTranslation(block: LayerId, text: string): Promise<Change> {
  return commands.booharuSetTranslation(block, text)
}

/** Commits `text` as `block`'s translation, as one undo step, as typing it in Story does: in place
 * of the text of the translation it has, or as its first, which `setTranslation` places in its
 * frame. A placeholder isn't a translation here, so typing into a block styled before it was
 * translated places the text too, in that style (P-d). Story's field and Explain this line's Use
 * this both commit through it. A failure shows its error, as every failed command's does. */
export function commitTranslation(block: BlockLayer, text: string): void {
  const first = !block.translation || isPlaceholder(block.translation)
  dispatch(first ? setTranslation : setText, block.id, text)
}
