'use client'

import type { WordRange } from '@/lib/booharu/words'
import { letteringOf } from '@/lib/document'
import type { Layer, Text } from '@koharu/ipc/protocol'

// Booharu's type bar helpers (S11-f, S11-g): the words the bar says its character settings apply
// to, quoted as the canvas quotes the characters it types, and the selected texts carrying word
// styles, which the whole-box Clear takes off.

/** The most grapheme clusters of the words the bar quotes, as koharu-canvas cuts its quote. */
const QUOTED = 80

/** What a line breaks at, each a space in the quote: Unicode's mandatory breaks, as koharu-canvas
 * reads them, CR LF counting as one. */
const LINE_BREAKS = /\r\n|[\n\r\v\f\u0085\u2028\u2029]/g

const graphemes = new Intl.Segmenter(undefined, { granularity: 'grapheme' })

/** `text` as the type bar quotes it, as koharu-canvas quotes the selected characters it types
 * (`Typed.selected`): each line break a space, cut to 80 grapheme clusters with "…" after. */
export function quoted(text: string): string {
  const flat = text.replace(LINE_BREAKS, ' ')
  let count = 0
  for (const { index } of graphemes.segment(flat)) {
    if (count === QUOTED) return `${flat.slice(0, index)}…`
    count += 1
  }
  return flat
}

/** The words `range` holds, chosen in the Story panel, as the type bar quotes them. */
export function wordsQuote(range: WordRange): string {
  const bytes = new TextEncoder().encode(range.text)
  return quoted(new TextDecoder().decode(bytes.subarray(range.start, range.end)))
}

/** Whether `text`'s characters carry a word style: a mark of their own on any of its spans, as
 * crates/booharu's `clear_all` reads them. */
export function carriesMarks(text: Text | null | undefined): boolean {
  return (text?.content ?? []).some((span) =>
    Object.values(span.attributes ?? {}).some((value) => value != null),
  )
}

/** The `targets` whose text carries a word style: a text layer's own, or a block's translation's;
 * a block with no translation carries none. */
export function markedTargets(targets: Layer[]): Layer[] {
  return targets.filter((layer) => carriesMarks(letteringOf(layer)))
}
