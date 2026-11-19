import type { Layer } from '@koharu/ipc/protocol'

/** The attribute namespace of every Booharu key on a layer, as `booharu_core::attrs` names it. Any
 * agent can write these keys through the scene's own edit tool, so a reader takes a bad value for
 * none, as the registry's readers do. */
const namespace = 'booharu'

/** `review_note` on a block or a text layer: the reviewer's note, or null when it has none. */
export function reviewNote(layer: Layer): string | null {
  const value = layer.attributes[namespace]?.review_note
  return typeof value === 'string' && value.trim() ? value : null
}

/** `read_note` on a block: Read chapter's note on its line, or null when it has none. */
export function readNote(layer: Layer): string | null {
  const value = layer.attributes[namespace]?.read_note
  return typeof value === 'string' && value.trim() ? value : null
}
