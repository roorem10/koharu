import { charLength, charToUtf16Offset, utf16ToCharOffset } from '@/lib/richText'
import type {
  GlyphBox,
  Point,
  RunStyle,
  RunStylePatch,
  StyleRun,
  Typography,
} from '@koharu/bridge/protocol'

export type XY = [number, number]
export type Quad = GlyphBox['quad']

export const emptyRunStyle: RunStyle = {
  font: null,
  font_weight: null,
  font_style: null,
  size_scale: null,
  color: null,
  fill_gradient: null,
  stroke_color: null,
  stroke_width: null,
  outer_stroke_color: null,
  outer_stroke_width: null,
}

export function stylePatch(
  set: Partial<RunStyle> = {},
  clear: RunStylePatch['clear'] = [],
): RunStylePatch {
  return { set: { ...emptyRunStyle, ...set }, clear }
}

/** One native edit for a typing chunk, including deletions and non-BMP characters. */
export function textChange(before: string, after: string) {
  const a = Array.from(before),
    b = Array.from(after)
  let start = 0
  while (start < a.length && start < b.length && a[start] === b[start]) start++
  let end = a.length,
    tail = b.length
  while (end > start && tail > start && a[end - 1] === b[tail - 1]) {
    end--
    tail--
  }
  return { start, end, insert: b.slice(start, tail).join('') }
}

const words = new Intl.Segmenter(undefined, { granularity: 'word' })
const graphemes = new Intl.Segmenter(undefined, { granularity: 'grapheme' })

export function wordAt(text: string, offset: number): [number, number] {
  const index = charToUtf16Offset(text, Math.min(offset, Math.max(0, charLength(text) - 1)))
  for (const word of words.segment(text)) {
    if (word.index <= index && index < word.index + word.segment.length) {
      return [
        utf16ToCharOffset(text, word.index),
        utf16ToCharOffset(text, word.index + word.segment.length),
      ]
    }
  }
  return [0, 0]
}

export function wordAtPoint(
  text: string,
  glyphs: GlyphBox[],
  point: Point,
  vertical: boolean,
): [number, number] {
  const glyph = glyphs.findLast((g) => {
    const sides = g.quad.map((a, i) => {
      const b = g.quad[(i + 1) % 4]
      return (b[0] - a[0]) * (point.y - a[1]) - (b[1] - a[1]) * (point.x - a[0])
    })
    return sides.every((n) => n >= 0) || sides.every((n) => n <= 0)
  })
  const offset = nearestOffset(glyph ? [glyph] : glyphs, point, vertical)
  return wordAt(text, glyph ? Math.min(glyph.end - 1, offset) : offset)
}

export function stepOffset(text: string, offset: number, delta: number, byWord = false): number {
  const boundaries = [0, ...(byWord ? words : graphemes).segment(text)].map((entry) =>
    typeof entry === 'number'
      ? entry
      : utf16ToCharOffset(text, entry.index + (delta > 0 ? entry.segment.length : 0)),
  )
  return delta < 0
    ? (boundaries.filter((n) => n < offset).at(-1) ?? 0)
    : (boundaries.find((n) => n > offset) ?? charLength(text))
}

const mix = (a: XY, b: XY, t: number): XY => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]

export function glyphEdge(glyph: GlyphBox, offset: number, vertical: boolean): [XY, XY] {
  const t = Math.max(0, Math.min(1, (offset - glyph.start) / (glyph.end - glyph.start)))
  const [a, b, c, d] = glyph.quad
  return vertical ? [mix(a, d, t), mix(b, c, t)] : [mix(a, b, t), mix(d, c, t)]
}

export function caretEdge(glyphs: GlyphBox[], offset: number, vertical: boolean): [XY, XY] | null {
  const glyph =
    glyphs.find((g) => g.start <= offset && offset < g.end) ??
    glyphs.findLast((g) => g.end <= offset) ??
    glyphs[0]
  return glyph ? glyphEdge(glyph, offset, vertical) : null
}

function segmentDistance(point: Point, [a, b]: [XY, XY]) {
  const dx = b[0] - a[0],
    dy = b[1] - a[1]
  const t = Math.max(
    0,
    Math.min(1, ((point.x - a[0]) * dx + (point.y - a[1]) * dy) / (dx * dx + dy * dy || 1)),
  )
  return Math.hypot(point.x - a[0] - dx * t, point.y - a[1] - dy * t)
}

export function nearestOffset(glyphs: GlyphBox[], point: Point, vertical: boolean): number {
  let distance = Infinity,
    offset = 0
  for (const glyph of glyphs) {
    for (let n = glyph.start; n <= glyph.end; n++) {
      const next = segmentDistance(point, glyphEdge(glyph, n, vertical))
      if (next < distance) {
        distance = next
        offset = n
      }
    }
  }
  return offset
}

export function selectionQuads(
  glyphs: GlyphBox[],
  start: number,
  end: number,
  vertical: boolean,
): Quad[] {
  return glyphs
    .filter((g) => g.start < end && g.end > start)
    .map((g) => {
      const [a, d] = glyphEdge(g, Math.max(start, g.start), vertical)
      const [b, c] = glyphEdge(g, Math.min(end, g.end), vertical)
      return [a, b, c, d]
    })
}

export function adjacentLine(
  glyphs: GlyphBox[],
  offset: number,
  delta: number,
  vertical: boolean,
): number {
  const glyph =
    glyphs.find((g) => g.start <= offset && offset < g.end) ??
    glyphs.findLast((g) => g.end <= offset) ??
    glyphs[0]
  if (!glyph) return offset
  const edge = glyphEdge(glyph, offset, vertical)
  const point = { x: (edge[0][0] + edge[1][0]) / 2, y: (edge[0][1] + edge[1][1]) / 2 }
  const target = glyphs.filter((g) => g.line === glyph.line + delta)
  return target.length ? nearestOffset(target, point, vertical) : offset
}

export function lineBoundary(
  glyphs: GlyphBox[],
  offset: number,
  end: boolean,
  length: number,
): number {
  const glyph =
    glyphs.find((g) => g.start <= offset && offset < g.end) ??
    glyphs.findLast((g) => g.end <= offset) ??
    glyphs[0]
  const line = glyphs.filter((g) => g.line === glyph?.line)
  return line.length
    ? end
      ? Math.max(...line.map((g) => g.end))
      : Math.min(...line.map((g) => g.start))
    : end
      ? length
      : 0
}

/** Include inherited gaps as well as runs, so mixed values never hide the box style. */
export function selectedStyles(
  runs: StyleRun[],
  start: number,
  end: number,
  base: Typography | null,
) {
  const defaults = {
    font: base?.preferred_font ?? 'CCWildWords',
    font_weight: base?.font_weight ?? 400,
    font_style: base?.font_style ?? 'normal',
    size_scale: 1,
    color: base?.color ?? [0, 0, 0, 255],
  }
  const samples = new Set([
    start,
    ...runs.flatMap((r) => [r.start, r.end]).filter((n) => n > start && n < end),
  ])
  const values = [...samples].map((n) => {
    const style = runs.find((r) => r.start <= n && n < r.end)?.style
    return Object.fromEntries(
      Object.entries(defaults).map(([key, value]) => [
        key,
        style?.[key as keyof RunStyle] ?? value,
      ]),
    )
  })
  const value = <K extends keyof typeof defaults>(key: K): (typeof defaults)[K] | null =>
    values.every((v) => JSON.stringify(v[key]) === JSON.stringify(values[0]?.[key]))
      ? (values[0]?.[key] as (typeof defaults)[K])
      : null
  return {
    font: value('font'),
    weight: value('font_weight'),
    italic: value('font_style'),
    scale: value('size_scale'),
    color: value('color'),
  }
}

export function frameQuad(frame: {
  x: number
  y: number
  width: number
  height: number
  angle_degrees: number
}): Quad {
  const cx = frame.x + frame.width / 2,
    cy = frame.y + frame.height / 2
  const angle = (frame.angle_degrees * Math.PI) / 180,
    cos = Math.cos(angle),
    sin = Math.sin(angle)
  return [
    [-1, -1],
    [1, -1],
    [1, 1],
    [-1, 1],
  ].map(([x, y]): XY => [
    cx + ((x * frame.width) / 2) * cos - ((y * frame.height) / 2) * sin,
    cy + ((x * frame.width) / 2) * sin + ((y * frame.height) / 2) * cos,
  ]) as Quad
}
