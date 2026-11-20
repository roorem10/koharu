import { describe, expect, it } from 'vitest'

import { carriesMarks, markedTargets, quoted, wordsQuote } from '@/lib/booharu/typebar'
import type { Layer, Span } from '@koharu/ipc/protocol'

import { text } from './typeBar'

/** A block whose translation is `content`, or with none. */
function block(id: string, content?: Span[]): Layer {
  return {
    id,
    name: id,
    visible: true,
    locked: false,
    opacity: 1,
    blend: 'normal',
    attributes: {},
    kind: 'block',
    source: { content: [{ insert: 'こんにちは' }] },
    ...(content ? { translation: { content, style: { chars: { size: 24 } } } } : {}),
  } as Layer
}

describe('Booharu type bar quote (S11-f)', () => {
  it('puts the words on one line, CR LF and every mandatory break a space each', () => {
    expect(quoted('Hello')).toBe('Hello')
    expect(quoted('Hello\nworld')).toBe('Hello world')
    expect(quoted('a\r\nb\rc\u2028d\u2029e\u000bf\u000cg\u0085h')).toBe('a b c d e f g h')
  })

  it('cuts at 80 grapheme clusters, as the canvas quotes typed words', () => {
    expect(quoted('魔'.repeat(100))).toBe(`${'魔'.repeat(80)}…`)
    // A thumbs-up with its skin tone is one cluster of two characters, four UTF-16 units.
    const thumbs = '\u{1F44D}\u{1F3FD}'
    expect(quoted(thumbs.repeat(81))).toBe(`${thumbs.repeat(80)}…`)
    expect(quoted(thumbs.repeat(80))).toBe(thumbs.repeat(80))
    expect(quoted(`${'é'.repeat(80)}x`)).toBe(`${'é'.repeat(80)}…`)
  })

  it("quotes the words chosen in Story by the range's UTF-8 bytes", () => {
    // 世 and 界 take three bytes each, and 😀 four.
    const range = { layer: 'text', start: 6, end: 14, text: '世界😀 hi\nthere' }
    expect(wordsQuote(range)).toBe('😀 hi ')
    expect(wordsQuote({ ...range, start: 0, end: 19 })).toBe('世界😀 hi there')
  })
})

describe('Booharu texts carrying word styles (S11-g)', () => {
  it('finds a mark of their own on any span, never a null one', () => {
    expect(carriesMarks({ content: [{ insert: 'Hi ' }, { insert: 'there' }] })).toBe(false)
    expect(carriesMarks({ content: [{ insert: 'Hi', attributes: {} }] })).toBe(false)
    expect(carriesMarks({ content: [{ insert: 'Hi', attributes: { weight: null } }] })).toBe(false)
    expect(
      carriesMarks({ content: [{ insert: 'Hi ' }, { insert: 'there', attributes: { size: 30 } }] }),
    ).toBe(true)
    expect(carriesMarks(null)).toBe(false)
  })

  it("reads a text layer's own text and a block's translation, none for an untranslated block", () => {
    const targets = [
      text('plain'),
      text('marked', { content: [{ insert: 'Hi', attributes: { italic: true } }] }),
      block('translated', [{ insert: 'Hey', attributes: { weight: 700 } }]),
      block('bare', [{ insert: 'Hey' }]),
      block('untranslated'),
    ]
    expect(markedTargets(targets).map((layer) => layer.id)).toEqual(['marked', 'translated'])
  })
})
