'use client'

import { create } from 'zustand'

import { findLayer, letteringOf, plainText } from '@/lib/document'
import i18n from '@/lib/i18n'
import {
  commands,
  type CharKey,
  type CharStyle,
  type Layer,
  type LayerId,
  type Span,
  type Text,
  type Typed,
  type WordChange,
} from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

// Booharu's word styles: the B and I rules that the type bar, Ctrl+B and Ctrl+I while typing, and
// the Story panel's word toolbar share, so the three agree; the run of a text's characters chosen
// in the Story panel; and the words edit, which styles such a run by hand.

/** From this weight up, characters read as bold, and B shows pressed (D12). */
export const BOLD_FROM = 600
/** The weight B gives. */
export const BOLD_WEIGHT = 700
/** The weight B takes characters back to: a regular face's. */
export const REGULAR_WEIGHT = 400

/** The keys whose values characters don't share, as the type bar holds them or the canvas tells
 * them. */
export type MixedKeys = ReadonlySet<CharKey> | readonly CharKey[]

/** Whether the characters don't share `key`'s value. */
export function isMixed(mixed: MixedKeys, key: CharKey): boolean {
  return 'has' in mixed ? mixed.has(key) : mixed.includes(key)
}

/** Whether B shows pressed for characters sharing `chars`: they share one weight, 600 or more. */
export function isBold(chars: CharStyle, mixed: MixedKeys): boolean {
  return !isMixed(mixed, 'weight') && (chars.weight ?? REGULAR_WEIGHT) >= BOLD_FROM
}

/** Whether I shows pressed for characters sharing `chars`: every one is italic. */
export function isItalic(chars: CharStyle, mixed: MixedKeys): boolean {
  return !isMixed(mixed, 'italic') && chars.italic === true
}

/** What pressing B gives characters, `bold` being whether it shows pressed: bold, 700, unless
 * every one already is, so mixed weights go bold too; then regular, 400. While typing, `typedOver`
 * is the weight of the text typed into: under 600, B off gives that weight back, since a typed
 * mark can't be taken off, so the characters look as if it were. */
export function boldPatch(bold: boolean, typedOver?: number | null): CharStyle {
  if (!bold) return { weight: BOLD_WEIGHT }
  return { weight: typedOver != null && typedOver < BOLD_FROM ? typedOver : REGULAR_WEIGHT }
}

/** What pressing I gives characters, `italic` being whether it shows pressed: italic, unless every
 * one already is. */
export function italicPatch(italic: boolean): CharStyle {
  return { italic: !italic }
}

/** What pressing B asks of the words edit for chosen words, `bold` being whether it shows
 * pressed: bold, or off, which takes the words' weight off, or sets regular where the text itself
 * is bold. */
export function boldWords(bold: boolean): WordChange {
  return { bold: !bold }
}

/** What pressing I asks of the words edit for chosen words, `italic` being whether it shows
 * pressed: italic, or off, which takes the words' italic off, or sets upright where the text
 * itself is italic. */
export function italicWords(italic: boolean): WordChange {
  return { italic: !italic }
}

/** The weight of the text being typed into, `layer` among `layers`, under its characters' marks:
 * a block's translation's or a text layer's own; none for a new text, a text with no weight of its
 * own, or a layer `layers` lacks. */
export function typedWeight(
  layers: Layer[] | undefined,
  layer: LayerId | null,
): number | undefined {
  if (!layers || layer === null) return undefined
  const placed = findLayer(layers, layer)
  return (placed && letteringOf(placed.layer)?.style?.chars?.weight) ?? undefined
}

/** What B gives the characters the canvas types, as the type bar's B and Ctrl+B give it, `layers`
 * being the shown page's. */
export function typedBold(typing: Typed, layers: Layer[] | undefined): CharStyle {
  return boldPatch(isBold(typing.chars, typing.mixed), typedWeight(layers, typing.layer))
}

/** What I gives the characters the canvas types, as the type bar's I and Ctrl+I give it. */
export function typedItalic(typing: Typed): CharStyle {
  return italicPatch(isItalic(typing.chars, typing.mixed))
}

/** The weights the type bar's list offers: `real`, the family's own of either style, with
 * regular, bold and `shown`, the weight shown, each once, lightest first. None is snapped to a
 * face, as the renderer makes a weight a family lacks (D12). */
export function weightChoices(real: Iterable<number>, shown: number): number[] {
  return [...new Set([...real, REGULAR_WEIGHT, BOLD_WEIGHT, shown])].sort((a, b) => a - b)
}

/** A run of a text's characters chosen in the Story panel, which its word toolbar and the type bar
 * style. */
export interface WordRange {
  /** The layer whose text the run is in: a block, whose translation it is, or a text layer. */
  layer: LayerId
  /** Where the run starts, in UTF-8 bytes of `text`, as the scene counts. */
  start: number
  /** Where it ends, exclusive, in UTF-8 bytes. */
  end: number
  /** The saved plain text the run was chosen in, which the words edit checks is still the text. */
  text: string
}

/** The run of characters chosen in the Story panel, or none, and the Story field it was chosen
 * in, which the word toolbar shows by; none while a press elsewhere in the sidebar hides the
 * toolbar and keeps the run, as Booharu 0.0.4's did. */
export const useWordRange = create<{
  range: WordRange | null
  field: HTMLTextAreaElement | null
}>(() => ({ range: null, field: null }))

/** Chooses `range`, its word toolbar showing by `field`, or with null lets the chosen run go. */
export function setWordRange(
  range: WordRange | null,
  field: HTMLTextAreaElement | null = null,
): void {
  useWordRange.setState({ range, field: range ? field : null })
}

/** Where the character at `index` of `text`, a UTF-16 index as a textarea's selection gives it,
 * starts in UTF-8 bytes, as the scene counts. An index inside a character that takes two UTF-16
 * units, as an emoji does, counts from that character's start, as 0.0.4's offsets did. */
export function utf8Offset(text: string, index: number): number {
  const before = text.charCodeAt(index - 1)
  const after = text.charCodeAt(index)
  const inside = before >= 0xd800 && before <= 0xdbff && after >= 0xdc00 && after <= 0xdfff
  return new TextEncoder().encode(text.slice(0, inside ? index - 1 : index)).length
}

/** The characters of `text` the run `range` holds, each with its own marks: its spans cut at the
 * run's ends, in UTF-8 bytes, under the text's own style, which the type bar and the word toolbar
 * read the run's values from. */
export function chosenText(text: Text, range: WordRange): Text {
  const encoder = new TextEncoder()
  const decoder = new TextDecoder()
  const content: Span[] = []
  let at = 0
  for (const span of text.content ?? []) {
    const bytes = encoder.encode(span.insert)
    const from = Math.max(range.start - at, 0)
    const to = Math.min(range.end - at, bytes.length)
    if (from < to) content.push({ ...span, insert: decoder.decode(bytes.subarray(from, to)) })
    at += bytes.length
  }
  return { ...text, content }
}

/** The run chosen in the Story panel, when the type bar styles it rather than whole texts: while
 * the canvas isn't typing, `targets`, the texts the bar shows, being only the run's, whose saved
 * text is still the one the run was chosen in. */
export function useChosenWords(targets: Layer[], typing: Typed | null): WordRange | null {
  const range = useWordRange((state) => state.range)
  if (!range || typing || targets.length !== 1) return null
  const [target] = targets
  const lettering = target ? letteringOf(target) : null
  return target?.id === range.layer && lettering && plainText(lettering) === range.text
    ? range
    : null
}

/** The presses that keep the chosen run, which the React handlers of the parts that style it mark:
 * the word toolbar's, the Story field's and the type bar's. React hands them the presses in the
 * popups they open too, which portals put elsewhere in the page, so those count as inside. */
const keeping = new WeakSet<Event>()

/** Marks a press, from a React pointerdown handler, as one that keeps the chosen run. */
export function keepWords(event: { nativeEvent: Event }): void {
  keeping.add(event.nativeEvent)
}

/** Whether a React handler marked `event`, a press, as keeping the chosen run. It's read after
 * the press has reached them: a document listener catching it first waits a turn. */
export function keepsWords(event: Event): boolean {
  return keeping.has(event)
}

/** The words edit's refusal of a run whose text changed since it was chosen. */
const TEXT_CHANGED = 'The text changed. Select the words again.'

function sameRange(left: WordRange, right: WordRange): boolean {
  return (
    left.layer === right.layer &&
    left.start === right.start &&
    left.end === right.end &&
    left.text === right.text
  )
}

/** Styles the run `range` holds as `change` says, through the words edit, as one undo step, and
 * answers whether it did. A refusal or failure shows a toast, with why, in the interface's
 * language; a text that changed since also lets the run go when it's still the one chosen, as its
 * words may be gone. */
export async function styleWords(range: WordRange, change: WordChange): Promise<boolean> {
  try {
    await commands.booharuStyleWords(range.layer, range.start, range.end, range.text, change)
    return true
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    if (message.includes(TEXT_CHANGED)) {
      toast.add({ type: 'info', title: i18n.t('booharu.words.textChanged') })
      useWordRange.setState((state) =>
        state.range && sameRange(state.range, range) ? { range: null, field: null } : state,
      )
    } else {
      toast.add({ type: 'error', title: i18n.t('booharu.words.failed', { message }) })
    }
    return false
  }
}
