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

/** `auto`'s `typed` on a block: whether its translation was typed by hand, a first translation
 * written on the canvas, in Story or through Booharu's agent tools, with the marker in the same undo
 * step, which keeps it a hand edit once cleared, as booharu-core's `Auto::typed` reads the record.
 * A record that isn't an object, or a marker that isn't `true`, reads as none. */
export function typedByHand(layer: Layer): boolean {
  const auto = layer.attributes[namespace]?.auto
  return typeof auto === 'object' && auto !== null && !Array.isArray(auto) && auto.typed === true
}

/** `original_scan` on an image layer: whether it's the page's Original scan, the hidden, locked
 * layer Fix page levels keeps the scan in as it came, as booharu-core's `is_original_scan` reads
 * it. */
export function isOriginalScan(layer: Layer): boolean {
  return layer.kind === 'image' && layer.attributes[namespace]?.original_scan === true
}

/** `layers` without the Original scan, as Select all takes them: the scan stays hidden and locked
 * under the page, so a Delete after Select all removes the rest, which the lock would refuse. */
export function exceptOriginalScan(layers: Layer[]): Layer[] {
  return layers.filter((layer) => !isOriginalScan(layer))
}
