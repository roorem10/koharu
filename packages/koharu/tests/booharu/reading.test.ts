import { describe, expect, it } from 'vitest'

import {
  cantRead,
  isLocal,
  isPagesPerRequest,
  readerOf,
  readsByDefault,
} from '@/lib/booharu/reading'
import { addPreset, inRunOrder, sameStages, tickPresetStage, type Runs } from '@/lib/booharu/runs'

import { haikyu } from './context'
import { chapterState, gemma, gpt, indexTranslate, models, textOnly } from './reading'
import { builtInRuns } from './settings'

const runs = (): Runs => structuredClone(builtInRuns) as Runs

describe('Booharu reading rules', () => {
  it('reads with a model that sees images and calls tools, as the desktop picks it', () => {
    expect(cantRead(models, gpt.reference)).toBeNull()
    expect(cantRead(models, textOnly.reference)).toBe('no_vision')
    expect(cantRead(models, indexTranslate.reference)).toBe('only_translates')
    // One of Koharu's own may name a quantization; a model the list lacks counts as able.
    expect(cantRead(models, 'koharu/index-translate-9b:Q4_K_M')).toBe('only_translates')
    expect(cantRead(models, 'ollama/qwen3:8b')).toBeNull()
    expect([isLocal(gemma.reference), isLocal(gpt.reference), isLocal('koharu')]).toEqual([
      true,
      false,
      false,
    ])

    // The saved model when it can read, else none, as a read then refuses rather than fall back.
    expect(readerOf(models, gemma.reference, indexTranslate.reference)).toBe(gemma.reference)
    expect(readerOf(models, textOnly.reference, gpt.reference)).toBeNull()
    // With none saved, the translation model when it can read.
    expect(readerOf(models, null, gpt.reference)).toBe(gpt.reference)
    expect(readerOf(models, null, indexTranslate.reference)).toBeNull()
    expect(readerOf(models, null, null)).toBeNull()
  })

  it('starts Analyze chapter on for a chapter with a series, no brief and a model to read it', () => {
    expect(readsByDefault(chapterState(), gpt.reference)).toBe(true)
    // A blank summary counts as no brief.
    expect(readsByDefault(chapterState({ summary: '  \n' }), gpt.reference)).toBe(true)
    expect(readsByDefault(chapterState({ summary: 'A match.' }), gpt.reference)).toBe(false)
    expect(readsByDefault(chapterState(null, null), gpt.reference)).toBe(false)
    expect(readsByDefault(chapterState(null, haikyu()), null)).toBe(false)
    expect(readsByDefault(undefined, gpt.reference)).toBe(false)
  })

  it('takes 1 to 40 pages a request, whole numbers only', () => {
    expect([1, 20, 40].every(isPagesPerRequest)).toBe(true)
    expect([null, 0, 41, 2.5, Number.NaN].some(isPagesPerRequest)).toBe(false)
  })
})

describe('Booharu presets with Analyze chapter', () => {
  it('keeps Analyze chapter before Translation, and only together with it', () => {
    expect(inRunOrder(['translate', 'read', 'detect'])).toEqual(['detect', 'read', 'translate'])
    expect(inRunOrder(['read', 'inpaint'])).toEqual(['inpaint'])
    expect(sameStages(['translate', 'read'], ['read', 'translate'])).toBe(true)
    expect(sameStages(['inpaint', 'translate', 'read'], ['inpaint', 'translate'])).toBe(false)

    const added = addPreset('Read first', ['translate', 'read', 'recognize'])(runs())
    expect(added.presets.at(-1)).toEqual({
      name: 'Read first',
      stages: ['recognize', 'read', 'translate'],
    })
    expect(addPreset('Clean', ['read', 'inpaint'])(runs()).presets.at(-1)!.stages).toEqual([
      'inpaint',
    ])

    // Translate + clean up: Analyze chapter goes on before Translation, and off with it.
    const reads = tickPresetStage('Translate + clean up', 'read', true)(runs())
    expect(reads.presets[1]!.stages).toEqual(['inpaint', 'read', 'translate'])
    const cleans = tickPresetStage('Translate + clean up', 'translate', false)(reads)
    expect(cleans.presets[1]!.stages).toEqual(['inpaint'])
    // Detect + OCR doesn't translate, so it can't read; and a preset keeps a stage besides Read.
    expect(tickPresetStage('Detect + OCR', 'read', true)(runs())).toEqual(runs())
    const translates = { ...runs(), presets: [{ name: 'T', stages: ['read', 'translate'] }] }
    expect(tickPresetStage('T', 'translate', false)(translates as Runs)).toEqual(translates)
  })
})
