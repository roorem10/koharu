import { describe, expect, it, vi } from 'vitest'

import {
  boldPatch,
  boldWords,
  chosenText,
  isBold,
  isItalic,
  italicPatch,
  italicWords,
  setWordRange,
  styleWords,
  typedBold,
  typedItalic,
  typedWeight,
  useWordRange,
  utf8Offset,
  weightChoices,
  type WordRange,
} from '@/lib/booharu/words'
import { commands, type Layer, type Text, type Typed } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

/** A text layer whose own style has `weight`, or none. */
function textLayer(id: string, weight?: number): Layer {
  return {
    id,
    name: id,
    visible: true,
    locked: false,
    opacity: 1,
    blend: 'normal',
    attributes: {},
    kind: 'text',
    content: [{ insert: 'Hello' }],
    style: { chars: weight === undefined ? { size: 24 } : { size: 24, weight } },
  }
}

describe('Booharu B and I rules', () => {
  it('shows B pressed only while every character shares a weight of 600 or more', () => {
    expect(isBold({ weight: 700 }, [])).toBe(true)
    expect(isBold({ weight: 600 }, new Set())).toBe(true)
    expect(isBold({ weight: 599 }, [])).toBe(false)
    expect(isBold({}, [])).toBe(false)
    expect(isBold({}, ['weight'])).toBe(false)
    expect(isBold({}, new Set(['weight'] as const))).toBe(false)
  })

  it('shows I pressed only while every character is italic', () => {
    expect(isItalic({ italic: true }, [])).toBe(true)
    expect(isItalic({ italic: false }, [])).toBe(false)
    expect(isItalic({}, [])).toBe(false)
    expect(isItalic({}, ['italic'])).toBe(false)
  })

  it('gives bold from off and from mixed weights, and regular or the typed text weight back', () => {
    expect(boldPatch(false)).toEqual({ weight: 700 })
    expect(boldPatch(isBold({}, ['weight']))).toEqual({ weight: 700 })
    expect(boldPatch(true)).toEqual({ weight: 400 })
    // Typing on a light text gives its own weight back; on a bold one, regular.
    expect(boldPatch(true, 300)).toEqual({ weight: 300 })
    expect(boldPatch(true, 400)).toEqual({ weight: 400 })
    expect(boldPatch(true, 700)).toEqual({ weight: 400 })
    expect(boldPatch(true, null)).toEqual({ weight: 400 })
    expect(boldPatch(false, 300)).toEqual({ weight: 700 })
  })

  it('gives italic unless every character is italic', () => {
    expect(italicPatch(isItalic({ italic: true }, []))).toEqual({ italic: false })
    expect(italicPatch(isItalic({ italic: false }, []))).toEqual({ italic: true })
    expect(italicPatch(isItalic({}, ['italic']))).toEqual({ italic: true })
  })

  it('asks the words edit for the same toggles', () => {
    expect(boldWords(true)).toEqual({ bold: false })
    expect(boldWords(false)).toEqual({ bold: true })
    expect(italicWords(true)).toEqual({ italic: false })
    expect(italicWords(false)).toEqual({ italic: true })
  })

  it("reads the typed text's own weight, which a new text has none of", () => {
    const layers = [textLayer('light', 300), textLayer('plain')]
    expect(typedWeight(layers, 'light')).toBe(300)
    expect(typedWeight(layers, 'plain')).toBeUndefined()
    expect(typedWeight(layers, null)).toBeUndefined()
    expect(typedWeight(layers, 'gone')).toBeUndefined()
    expect(typedWeight(undefined, 'light')).toBeUndefined()

    const typing = (layer: string | null, typed: Partial<Typed>): Typed => ({
      layer,
      chars: {},
      mixed: [],
      ...typed,
    })
    expect(typedBold(typing('light', { chars: { weight: 700 } }), layers)).toEqual({ weight: 300 })
    expect(typedBold(typing('plain', { chars: { weight: 700 } }), layers)).toEqual({ weight: 400 })
    expect(typedBold(typing(null, { chars: { weight: 700 } }), layers)).toEqual({ weight: 400 })
    expect(typedBold(typing('light', { mixed: ['weight'] }), layers)).toEqual({ weight: 700 })
    expect(typedItalic(typing('plain', { chars: { italic: true } }))).toEqual({ italic: false })
    expect(typedItalic(typing('plain', { mixed: ['italic'] }))).toEqual({ italic: true })
  })

  it("offers the family's weights with regular, bold and the one shown, never snapped", () => {
    expect(weightChoices([300, 500, 300], 550)).toEqual([300, 400, 500, 550, 700])
    expect(weightChoices([400], 400)).toEqual([400, 700])
    expect(weightChoices([], 900)).toEqual([400, 700, 900])
  })
})

describe('Booharu word ranges', () => {
  it('counts UTF-16 indices in UTF-8 bytes, emoji and CJK included', () => {
    expect(utf8Offset('Hello world', 6)).toBe(6)
    // é is two bytes, 世 and 界 three each, and 😀 four, though two UTF-16 units.
    expect(utf8Offset('café!', 4)).toBe(5)
    expect(utf8Offset('世界です', 2)).toBe(6)
    expect(utf8Offset('😀 hi', 2)).toBe(4)
    expect(utf8Offset('a😀b', 4)).toBe(6)
    // An index between an emoji's two UTF-16 units counts from the emoji's start.
    expect(utf8Offset('a😀b', 2)).toBe(1)
  })

  it('cuts the chosen characters out of their spans by UTF-8 bytes, each with its marks', () => {
    const text: Text = {
      content: [
        { insert: '世界' },
        { insert: '😀 hi', attributes: { weight: 700 } },
        { insert: ' end', attributes: { italic: true } },
      ],
      style: { chars: { size: 24 } },
    }
    const range = (start: number, end: number): WordRange => ({
      layer: 'text',
      start,
      end,
      text: '世界😀 hi end',
    })

    // 界 is bytes 3 to 6, 😀 6 to 10, and the space and h 10 to 12.
    expect(chosenText(text, range(3, 12))).toEqual({
      content: [{ insert: '界' }, { insert: '😀 h', attributes: { weight: 700 } }],
      style: { chars: { size: 24 } },
    })
    // A run that ends where a span starts, or starts where one ends, takes nothing of it.
    expect(chosenText(text, range(0, 6)).content).toEqual([{ insert: '世界' }])
    expect(chosenText(text, range(13, 17)).content).toEqual([
      { insert: ' end', attributes: { italic: true } },
    ])
  })

  it('shows the toolbar by the field the words were chosen in, never once they go', () => {
    const field = document.createElement('textarea')
    setWordRange({ layer: 'text', start: 0, end: 5, text: 'Hello' }, field)
    expect(useWordRange.getState().field).toBe(field)

    setWordRange(null, field)
    expect(useWordRange.getState()).toEqual({ range: null, field: null })
  })

  it("styles the chosen words through the words edit with the text they're in", async () => {
    const styled = vi.spyOn(commands, 'booharuStyleWords').mockResolvedValue(null)
    const range: WordRange = { layer: 'block', start: 6, end: 11, text: 'Hello world' }
    setWordRange(range)

    await expect(styleWords(range, { emphasis: true })).resolves.toBe(true)
    expect(styled).toHaveBeenCalledExactlyOnceWith('block', 6, 11, 'Hello world', {
      emphasis: true,
    })
    expect(useWordRange.getState().range).toEqual(range)
  })

  it('lets the words go when their text changed, and keeps them on another refusal', async () => {
    const shown = vi.spyOn(toast, 'add')
    const styled = vi
      .spyOn(commands, 'booharuStyleWords')
      .mockRejectedValueOnce('block 7 has no translation, so it has no words to style')
      .mockRejectedValueOnce(new Error('The text changed. Select the words again.'))
    const range: WordRange = { layer: 'block', start: 0, end: 5, text: 'Hello' }
    setWordRange({ ...range }, document.createElement('textarea'))

    // Each says why, in the interface's language: the refusal's own words after a failure's.
    await expect(styleWords(range, { bold: true })).resolves.toBe(false)
    expect(useWordRange.getState().range).toEqual(range)
    expect(shown).toHaveBeenLastCalledWith({
      type: 'error',
      title: "Couldn't style the words: block 7 has no translation, so it has no words to style",
    })
    await expect(styleWords(range, { bold: true })).resolves.toBe(false)
    expect(useWordRange.getState()).toEqual({ range: null, field: null })
    expect(shown).toHaveBeenLastCalledWith({
      type: 'info',
      title: 'The text changed. Select the words again.',
    })
    expect(styled).toHaveBeenCalledTimes(2)
    expect(shown).toHaveBeenCalledTimes(2)
  })

  it('keeps other words chosen since when the text changed under the old ones', async () => {
    vi.spyOn(commands, 'booharuStyleWords').mockRejectedValue(
      'The text changed. Select the words again.',
    )
    const chosen: WordRange = { layer: 'block', start: 0, end: 2, text: 'Hi there' }
    setWordRange(chosen)

    await styleWords({ layer: 'block', start: 0, end: 5, text: 'Hello' }, { italic: true })
    expect(useWordRange.getState().range).toEqual(chosen)
  })

  it('shows no toast for words it styles', async () => {
    const shown = vi.spyOn(toast, 'add')
    vi.spyOn(commands, 'booharuStyleWords').mockResolvedValue(null)

    await styleWords({ layer: 'block', start: 0, end: 2, text: 'Hi' }, { clear: true })
    expect(shown).not.toHaveBeenCalled()
  })
})
