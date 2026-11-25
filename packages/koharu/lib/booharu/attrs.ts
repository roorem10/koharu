import type { Layer, Value } from '@koharu/ipc/protocol'

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
 * step, which keeps it a hand edit once cleared, read as booharu-core's `Auto::of` reads it: only
 * from a record that reads whole (`readsAsAuto`), as the desktop takes a record it can't read for
 * none, marker and all, so a block the desktop reads as a placeholder reads as one here too.
 * `tests/booharu/auto-records.json` holds both readers to the same answers. */
export function typedByHand(layer: Layer): boolean {
  const auto = layer.attributes[namespace]?.auto
  return readsAsAuto(auto) && auto.typed === true
}

/** Whether `value` reads as booharu-core's `Auto`, the hand-edit record `auto`, as `attrs::get`
 * reads it: an object whose `origin`, when there, is `automatic` or `agent`; whose `source` and
 * `text`, when not null, are hashes of 16 lowercase hex digits; whose `style`, when not null, reads
 * as an `AutoStyle` (`readsAsStyle`); and whose `kept`, `split` and `typed`, when there, are
 * booleans. It takes any other key, `marks` among them, whatever it holds, as booharu-core does.
 * A few forms booharu-core takes and Booharu never writes read as none here, such as a record or
 * a style given as an array or a name given as an object, so this may miss a marker but never
 * reads one booharu-core doesn't, except where JSON can't tell: a weight written with a point,
 * such as `400.0`, which booharu-core refuses, reaches the interface as 400. */
function readsAsAuto(value: Value | undefined): value is { [key in string]: Value } {
  if (!isObject(value)) return false
  const { origin, source, text, style, kept, split, typed } = value
  return (
    (origin === undefined || origin === 'automatic' || origin === 'agent') &&
    [source, text].every((hash) => hash == null || (typeof hash === 'string' && HASH.test(hash))) &&
    (style == null || readsAsStyle(style)) &&
    [kept, split, typed].every((flag) => flag === undefined || typeof flag === 'boolean')
  )
}

/** `styles::text_hash`'s form: 16 lowercase hex digits. */
const HASH = /^[0-9a-f]{16}$/

/** Whether `value` reads as booharu-core's `AutoStyle`, the style values an automatic step left on
 * a translation: an object whose keys, each null when unset, hold a family name, a weight from 0 to
 * 65535, an italic flag, colours of four components (`Rgba`), widths and letter spacing, and a
 * gradient fill (`readsAsGradient`). It takes keys it doesn't know. */
function readsAsStyle(value: Value): boolean {
  if (!isObject(value)) return false
  const unsetOr = (key: string, reads: (value: Value) => boolean) =>
    value[key] == null || reads(value[key])
  return (
    unsetOr('family', (family) => typeof family === 'string') &&
    unsetOr(
      'weight',
      (weight) =>
        typeof weight === 'number' && Number.isInteger(weight) && weight >= 0 && weight <= 65535,
    ) &&
    unsetOr('italic', (italic) => typeof italic === 'boolean') &&
    ['fill', 'stroke', 'outer_stroke'].every((key) => unsetOr(key, isRgba)) &&
    ['stroke_width', 'outer_stroke_width', 'tracking'].every((key) =>
      unsetOr(key, (number) => typeof number === 'number'),
    ) &&
    unsetOr('fill_gradient', readsAsGradient)
  )
}

/** Whether `value` reads as koharu-scene's `Gradient`, which takes no key it doesn't know: `kind`
 * `linear` or `radial`, an `angle` that is a number when there, and 2 to 8 `stops`, each an
 * `offset` from 0 to 1, never below the one before it, and a `color`, with no other key. */
function readsAsGradient(value: Value): boolean {
  if (!isObject(value) || !onlyKeys(value, ['kind', 'angle', 'stops'])) return false
  const { kind, angle, stops } = value
  if (kind !== 'linear' && kind !== 'radial') return false
  if (angle !== undefined && typeof angle !== 'number') return false
  if (!Array.isArray(stops) || stops.length < 2 || stops.length > 8) return false
  let last = 0
  return stops.every((stop) => {
    if (!isObject(stop) || !onlyKeys(stop, ['offset', 'color']) || !isRgba(stop.color)) return false
    const { offset } = stop
    if (typeof offset !== 'number' || offset < last || offset > 1) return false
    last = offset
    return true
  })
}

/** koharu-scene's `Rgba`: four numbers. */
function isRgba(value: Value): boolean {
  return (
    Array.isArray(value) && value.length === 4 && value.every((part) => typeof part === 'number')
  )
}

function isObject(value: Value | undefined): value is { [key in string]: Value } {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function onlyKeys(value: { [key in string]: Value }, keys: string[]): boolean {
  return Object.keys(value).every((key) => keys.includes(key))
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
