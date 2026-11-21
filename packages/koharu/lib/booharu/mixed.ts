'use client'

import { paintUnit } from '@/lib/booharu/paint'
import { toRgba } from '@/lib/color'
import { isBlockLayer, isTextLayer, letteringOf } from '@/lib/document'
import {
  commands,
  type Align,
  type Change,
  type CharStyle,
  type Layer,
  type Op,
  type StrokeMethod,
  type Text,
  type TextStyle,
  type Value,
} from '@koharu/ipc/protocol'

// Booharu's values that the texts selected in the type bar don't share (S11-h): what their
// paragraphs share, each from its own lettering, as the bar's style shows only the first text's;
// and the style edit that gives each text a patch of its own, so a value that differs and isn't
// edited stays each text's own.

/** The line spacing Koharu's renderer sets a text at where its style gives none. */
export const DEFAULT_LINE_HEIGHT = 1.2

/** A value every target shares, or that they don't. */
export type Shared<T> = { mixed: false; value: T } | { mixed: true }

/** What the targets' paragraphs share, as the type bar shows each value. */
export type ParagraphShare = {
  /** The alignment: a vertical text's start, and a horizontal one's centre, where it sets none. */
  align: Shared<Align>
  /** Whether the text runs in vertical columns. */
  vertical: Shared<boolean>
  /** Whether the frame fits its text by the best size, the size field's Auto; point text has no
   * fit. The other fits all show the size set, so they don't differ here. */
  fit: Shared<boolean>
  /** The line spacing, a multiple of each line's largest size: Koharu's 1.2 where the text sets
   * none. */
  line_height: Shared<number>
  /** How the borders are drawn: expansion, Koharu's border, where the text sets no method. */
  stroke_method: Shared<StrokeMethod>
}

/** What `targets` share of their paragraphs, each value from each target's own lettering: a text
 * layer's, or a block's translation's, as it's drawn, so a value left unset and its default don't
 * differ. With no target, nothing differs and each value is a new text's. */
export function paragraphShare(targets: Layer[]): ParagraphShare {
  const texts = targets.flatMap((layer) => letteringOf(layer) ?? [])
  return {
    align: share(
      texts.map((text) => text.style?.align ?? (isVertical(text) ? 'start' : 'center')),
      'center',
    ),
    vertical: share(texts.map(isVertical), false),
    fit: share(texts.map(fitsBest), false),
    line_height: share(
      texts.map((text) => text.style?.line_height ?? DEFAULT_LINE_HEIGHT),
      DEFAULT_LINE_HEIGHT,
    ),
    stroke_method: share(
      texts.map((text) => text.style?.stroke_method ?? 'expansion'),
      'expansion',
    ),
  }
}

/** Styles each of `targets` with its own patch, from `patchFor`, or leaves it be where that gives
 * none, in one edit labelled as the type bar's other style edits, so one undo step. A target's ops
 * are those `setStyle` (lib/scene.ts) writes for it: its lettering's style merged with the patch,
 * and, for a character key the patch sets, its characters' own marks of that key taken off, both
 * paint keys for either. A selection whose values differ thus keeps each text's own of every value
 * not edited. */
export function setEach(
  targets: Layer[],
  patchFor: (layer: Layer) => TextStyle | null,
): Promise<Change> {
  return commands.edit({
    label: 'set_style',
    ops: targets
      .filter((layer) => isTextLayer(layer) || isBlockLayer(layer))
      .flatMap((layer) => {
        const style = patchFor(layer)
        return style ? styleOps(layer, style) : []
      }),
  })
}

/** The keys of the borders' colours and widths: the first border's and the second's. */
export type BorderKey = 'stroke' | 'stroke_width' | 'outer_stroke' | 'outer_stroke_width'

/** What a text with no border colour or width of its own takes when the border's other value is
 * set, as the type bar gives it: a white first border 1.5 wide, and a black second one 3 wide. */
const BORDER_DEFAULTS: CharStyle = {
  stroke: toRgba('#FFFFFF'),
  stroke_width: 1.5,
  outer_stroke: toRgba('#000000'),
  outer_stroke_width: 3,
}

/** What a border edit adds to `layer`'s own patch, `key` being the colour or the width that goes
 * with the value set: the type bar's default where the layer's characters have none, no colour or
 * no width, as a block with no translation yet, and nothing where they share one of their own, or
 * differ, so each keeps its own. */
export function borderCompanion(layer: Layer, key: BorderKey): CharStyle {
  const text = letteringOf(layer)
  const own = text ? charShare(text, key) : { mixed: false as const, value: undefined }
  if (own.mixed) return {}
  const width = key === 'stroke_width' || key === 'outer_stroke_width'
  const has = width ? typeof own.value === 'number' && own.value > 0 : own.value != null
  return has ? {} : { [key]: BORDER_DEFAULTS[key] }
}

/** The ops `setStyle` writes to style `layer` with `style`: either paint key takes both off the
 * marks, a fill and a gradient fill being one paint (S12-c). */
function styleOps(layer: Layer, style: TextStyle): Op[] {
  const keys = paintUnit(Object.keys(style.chars ?? {}) as (keyof CharStyle)[])
  const patch: Value = isBlockLayer(layer) ? { translation: { style } } : { style }
  const restyle: Op = { op: 'update_layer', layer: layer.id, patch }
  const spans = letteringOf(layer)?.content ?? []
  const marked = spans.some((span) => keys.some((key) => span.attributes?.[key] != null))
  if (!marked) return [restyle]
  const unmarked = spans.map(({ insert, attributes }) => ({
    insert,
    attributes: Object.fromEntries(
      Object.entries(attributes ?? {}).filter(([key]) => !keys.includes(key as keyof CharStyle)),
    ),
  }))
  return [restyle, { op: 'set_rich_text', layer: layer.id, spans: unmarked }]
}

/** What every character of `text` has for `key`, its own mark over the text's style, as the type
 * bar's sharedChars reads them, or that they differ. */
function charShare(text: Text, key: BorderKey): Shared<CharStyle[BorderKey]> {
  const base = text.style?.chars?.[key]
  const spans = text.content?.length ? text.content : [{ insert: '' }]
  return share(
    spans.map((span) => span.attributes?.[key] ?? base),
    base,
  )
}

/** The value every one of `values` has, `empty` where there are none, or that they differ. */
function share<T>(values: T[], empty: T): Shared<T> {
  const [first = empty, ...rest] = values
  const same = (value: T) => JSON.stringify(value ?? null) === JSON.stringify(first ?? null)
  return rest.every(same) ? { mixed: false, value: first } : { mixed: true }
}

function isVertical(text: Text): boolean {
  return text.direction === 'vertical'
}

/** Whether `text`'s frame fits it by the best size, as the type bar's Auto shows. */
function fitsBest(text: Text): boolean {
  const frame = text.frame ?? 'point'
  return typeof frame === 'object' && (frame.box ?? frame.path)?.fit === 'best'
}
