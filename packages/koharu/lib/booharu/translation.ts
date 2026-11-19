'use client'

import { dispatch } from '@/lib/backend'
import type { BlockLayer } from '@/lib/document'
import { setText } from '@/lib/scene'
import { commands, type Change, type LayerId } from '@koharu/ipc/protocol'

/** Booharu: a block's first translation, typed in the Story panel (#128). The scene's `set_text`
 * alone gives a block without a translation point text at the page's top-left corner, so the
 * desktop command sets the text and writes the translation as the Translation stage writes one it
 * places: in the block's frame, in the target language, with the block's clean-up shown, all in
 * one undo step. */
export function setTranslation(block: LayerId, text: string): Promise<Change> {
  return commands.booharuSetTranslation(block, text)
}

/** Commits `text` as `block`'s translation, as one undo step, as typing it in Story does: in place
 * of the text of the translation it has, or as its first, which `setTranslation` places in its
 * frame. Story's field and Explain this line's Use this both commit through it. A failure shows its
 * error, as every failed command's does. */
export function commitTranslation(block: BlockLayer, text: string): void {
  dispatch(block.translation ? setText : setTranslation, block.id, text)
}
