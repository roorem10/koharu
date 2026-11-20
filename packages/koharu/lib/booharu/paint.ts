'use client'

import type { CharStyle, Gradient, Rgba, Text } from '@koharu/ipc/protocol'

// Booharu's paint (S12-c): a character's fill and its gradient fill are one paint, as
// koharu-scene's CharStyle::or merges a span's marks over its text's style, so a word given a
// colour shows it in a gradient box; the gradient the type bar's Gradient fill switch seeds from a
// colour; and the edits of a gradient's two ends, which keep each end's alpha.

/** The colour a character with no fill draws in, as the renderer gives it. */
export const BLACK: Rgba = [0, 0, 0, 1]

/** The keys of a character's paint, which a style edit setting either takes off the marks
 * together. */
export const PAINT_KEYS = ['fill', 'fill_gradient'] as const

/** How much lighter, or darker, a seeded gradient's end is than its start. */
const SEED_SHADE = 0.6

/** The keys whose values characters don't share, as the type bar holds them or the canvas tells
 * them. */
type MixedKeys = ReadonlySet<string> | readonly string[]

function has(mixed: MixedKeys, key: keyof CharStyle): boolean {
  return 'has' in mixed ? mixed.has(key) : mixed.includes(key)
}

/** Whether `key` is one of a character's paint keys. */
export function isPaintKey(key: string): boolean {
  return (PAINT_KEYS as readonly string[]).includes(key)
}

/** `keys`, the character keys a style edit sets, with both paint keys wherever either is there,
 * so the edit takes a fill and a gradient fill off the characters' marks together. */
export function paintUnit<Key extends string>(keys: Key[]): (Key | (typeof PAINT_KEYS)[number])[] {
  return keys.some(isPaintKey) ? [...new Set([...keys, ...PAINT_KEYS])] : keys
}

/** `own`'s values, with `base`'s where it has none, as koharu-scene's `CharStyle::or` gives a
 * span's marks over its text's style: a fill in `own` with no gradient beside it hides the gradient
 * `base` has, and a gradient wins over a fill beside it. */
export function charsOr(own: CharStyle, base: CharStyle): CharStyle {
  const style: CharStyle = { ...base }
  for (const [key, value] of Object.entries(own)) {
    if (value != null) Object.assign(style, { [key]: value })
  }
  if (own.fill != null && own.fill_gradient == null) delete style.fill_gradient
  return style
}

/** The style of each run of `text`'s characters: each span's marks over the text's own style, as
 * the renderer draws them; the text's style alone when it has no characters. */
export function spanStyles(text: Text): CharStyle[] {
  const base = text.style?.chars ?? {}
  const spans = text.content?.length ? text.content : [{ insert: '' }]
  return spans.map((span) => charsOr(span.attributes ?? {}, base))
}

/** The flat colour of a character styled `style`, under any gradient it shows: its fill, or the
 * renderer's black. */
export function flatColor(style: CharStyle): Rgba {
  return style.fill ?? BLACK
}

/** What characters show: a gradient, or a flat colour. */
export type Paint = { gradient: Gradient } | { color: Rgba }

/** The paint every character sharing `chars` shows, `mixed` being the keys they don't share: the
 * gradient they share, or the flat colour, or none when they show different paints, some a
 * gradient and some not, or different gradients or colours. A colour differing under one gradient
 * doesn't count. */
export function sharedPaint(chars: CharStyle, mixed: MixedKeys): Paint | null {
  if (has(mixed, 'fill_gradient')) return null
  if (chars.fill_gradient) return { gradient: chars.fill_gradient }
  return has(mixed, 'fill') ? null : { color: flatColor(chars) }
}

/** Whether characters show a gradient: every one of them, some, or none. */
export type GradientShown = 'all' | 'some' | 'none'

/** Whether the characters sharing `chars` show a gradient, `mixed` being the keys they don't
 * share and `styles` each one's own style, when known, which tells every one showing a different
 * gradient from only some showing one. */
export function gradientShown(
  chars: CharStyle,
  mixed: MixedKeys,
  styles: CharStyle[] | null,
): GradientShown {
  if (!has(mixed, 'fill_gradient')) return chars.fill_gradient ? 'all' : 'none'
  return styles?.length && styles.every((style) => style.fill_gradient) ? 'all' : 'some'
}

/** The flat colour `text`'s characters show under any gradient: the fill every one has, or, where
 * they differ, the text's own; black where it sets none. */
export function ownColor(text: Text): Rgba {
  const [first = BLACK, ...rest] = spanStyles(text).map(flatColor)
  return rest.every((color) => sameColor(color, first)) ? first : flatColor(text.style?.chars ?? {})
}

/** The flat colour characters sharing `chars` show, `mixed` being the keys they don't share: the
 * fill they share, or, where they differ, `base`'s, their text's own; black where it sets none. */
export function sharedColor(chars: CharStyle, mixed: MixedKeys, base?: CharStyle): Rgba {
  return flatColor(has(mixed, 'fill') ? (base ?? {}) : chars)
}

/** Whether `color` reads as light: its luminance over a half, as the colour swap judges it. */
export function isLight([red, green, blue]: Rgba): boolean {
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue > 0.5
}

/** `color` moved `amount` of the way to white, each channel to a 255th, its alpha kept. */
export function lighter(color: Rgba, amount = SEED_SHADE): Rgba {
  return shade(color, (channel) => channel + (1 - channel) * amount)
}

/** `color` moved `amount` of the way to black, each channel to a 255th, its alpha kept. */
export function darker(color: Rgba, amount = SEED_SHADE): Rgba {
  return shade(color, (channel) => channel * (1 - amount))
}

/** The gradient Gradient fill seeds from `color`, a target's flat colour: linear at 90°, bottom to
 * top, from the colour to one 60% lighter, or 60% darker when the colour is light. */
export function seedGradient(color: Rgba): Gradient {
  return {
    kind: 'linear',
    angle: 90,
    stops: [
      { offset: 0, color },
      { offset: 1, color: isLight(color) ? darker(color) : lighter(color) },
    ],
  }
}

/** Which end of a gradient: its first stop or its last. */
export type GradientEnd = 'start' | 'end'

/** The colour of `gradient`'s first stop, or its last. */
export function endColor(gradient: Gradient, end: GradientEnd): Rgba {
  const stop = end === 'start' ? gradient.stops[0] : gradient.stops.at(-1)
  return stop?.color ?? BLACK
}

/** `gradient` with its first stop, or its last, coloured `color`, the stop keeping its own alpha,
 * as the colour well picks no alpha. */
export function withEndColor(gradient: Gradient, end: GradientEnd, color: Rgba): Gradient {
  const at = end === 'start' ? 0 : gradient.stops.length - 1
  return {
    ...gradient,
    stops: gradient.stops.map((stop, index) =>
      index === at ? { ...stop, color: [color[0], color[1], color[2], stop.color[3]] } : stop,
    ),
  }
}

/** `angle`, any number of degrees, as the same direction from -180 to 180. */
export function normalAngle(angle: number): number {
  const turned = (((angle + 180) % 360) + 360) % 360
  return turned === 0 && angle > 0 ? 180 : turned - 180
}

function shade(color: Rgba, channel: (value: number) => number): Rgba {
  const step = (value: number) => Math.round(Math.min(1, Math.max(0, channel(value))) * 255) / 255
  return [step(color[0]), step(color[1]), step(color[2]), color[3]]
}

function sameColor(left: Rgba, right: Rgba): boolean {
  return left.every((channel, index) => channel === right[index])
}
