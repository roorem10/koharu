'use client'

import { commands, type Change, type LayerId } from '@koharu/ipc/protocol'

/** Booharu: a block's first translation, typed in the Story panel (#128). The scene's `set_text`
 * alone gives a block without a translation point text at the page's top-left corner, so the
 * desktop command sets the text and writes the translation as the Translation stage writes one it
 * places: in the block's frame, in the target language, with the block's clean-up shown, all in
 * one undo step. */
export function setTranslation(block: LayerId, text: string): Promise<Change> {
  return commands.booharuSetTranslation(block, text)
}
