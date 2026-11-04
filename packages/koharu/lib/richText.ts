// Word styles (batch 3) store ranges as Unicode code point offsets (Rust `char`s),
// while JavaScript strings and DOM selections count UTF-16 code units.
// Convert at the edge with these helpers so the frontend and backend agree.

/** UTF-16 offset in `text` -> code point offset. Offsets inside a surrogate pair round down. */
export function utf16ToCharOffset(text: string, utf16: number): number {
  let chars = 0
  let index = 0
  const limit = Math.max(0, Math.min(utf16, text.length))
  while (index < limit) {
    const code = text.charCodeAt(index)
    const pair = code >= 0xd800 && code <= 0xdbff && index + 1 < text.length
    if (pair && index + 1 >= limit) break
    index += pair ? 2 : 1
    chars += 1
  }
  return chars
}

/** Code point offset in `text` -> UTF-16 offset. Offsets past the end clamp to `text.length`. */
export function charToUtf16Offset(text: string, chars: number): number {
  let index = 0
  for (let count = 0; count < chars && index < text.length; count += 1) {
    const code = text.charCodeAt(index)
    index += code >= 0xd800 && code <= 0xdbff && index + 1 < text.length ? 2 : 1
  }
  return index
}

/** Number of code points in `text`. */
export function charLength(text: string): number {
  return utf16ToCharOffset(text, text.length)
}

/** Words selected on the page in the on-page editor (WO-11D), read by the panel (WO-11E). */
export type RichTextSelection = {
  layer: string
  /** Code point offsets, start < end. */
  start: number
  end: number
}
