import type { TFunction } from 'i18next'
import { describe, expect, it } from 'vitest'

import {
  addPreset,
  deletePreset,
  isRetries,
  presetLabel,
  presetNameError,
  renamePreset,
  setRuns,
  tickPresetStage,
  type Runs,
} from '@/lib/booharu/runs'
import type { RunPreset } from '@koharu/ipc/protocol'

import { builtInRuns } from './settings'

/** A translation function that marks each string it translates, with its values. */
const t = ((key: string, values?: Record<string, unknown>) =>
  values ? `[${key} ${JSON.stringify(values)}]` : `[${key}]`) as unknown as TFunction

const runs = (): Runs => structuredClone(builtInRuns) as Runs

describe('Booharu run presets', () => {
  it('shows the built-in names translated only while their names and stages are the built-in ones', () => {
    const [detect, translate] = builtInRuns.presets as RunPreset[]
    expect(presetLabel(detect!, t)).toBe('[booharu.runs.presets.builtIn.detectOcr]')
    expect(presetLabel(translate!, t)).toBe('[booharu.runs.presets.builtIn.translateCleanup]')
    // In another order, the stages are still the built-in ones.
    expect(presetLabel({ ...detect!, stages: ['recognize', 'detect'] }, t)).toBe(
      '[booharu.runs.presets.builtIn.detectOcr]',
    )
    expect(presetLabel({ ...detect!, stages: ['detect'] }, t)).toBe('Detect + OCR')
    expect(presetLabel({ ...translate!, name: 'Translate + clean up ' }, t)).toBe(
      'Translate + clean up ',
    )
    expect(presetLabel({ name: 'Clean only', stages: ['inpaint'] }, t)).toBe('Clean only')
  })

  it('refuses a blank name, or one another preset has or shows, and lets a preset keep its own', () => {
    const presets = builtInRuns.presets as RunPreset[]
    expect(presetNameError('  ', presets, t)).toBe('[booharu.runs.presets.blank]')
    expect(presetNameError(' Detect + OCR ', presets, t)).toBe(
      '[booharu.runs.presets.duplicate {"name":"Detect + OCR"}]',
    )
    // The name a built-in preset shows, in the language shown, is taken too.
    expect(presetNameError('[booharu.runs.presets.builtIn.translateCleanup]', presets, t)).toBe(
      '[booharu.runs.presets.duplicate {"name":"[booharu.runs.presets.builtIn.translateCleanup]"}]',
    )
    expect(presetNameError('Clean only', presets, t)).toBeNull()
    expect(presetNameError('Detect + OCR', presets, t, 'Detect + OCR')).toBeNull()
  })

  it('makes each change to the presets it was made on, found by name', () => {
    const added = addPreset('  Read + translate ', ['translate', 'recognize'])(runs())
    expect(added.presets.at(-1)).toEqual({
      name: 'Read + translate',
      stages: ['recognize', 'translate'],
    })

    // Another writer has removed the first preset since: the rename still finds its own.
    const others = { ...added, presets: added.presets.slice(1) }
    expect(
      renamePreset('Read + translate', ' Read ')(others).presets.map(({ name }) => name),
    ).toEqual(['Translate + clean up', 'Read'])

    const ticked = tickPresetStage('Detect + OCR', 'inpaint', true)(runs())
    expect(ticked.presets[0]!.stages).toEqual(['detect', 'recognize', 'inpaint'])
    const unticked = tickPresetStage('Detect + OCR', 'detect', false)(ticked)
    expect(unticked.presets[0]!.stages).toEqual(['recognize', 'inpaint'])
    // A preset keeps at least one stage.
    const recognize = tickPresetStage('Detect + OCR', 'detect', false)(runs())
    const one = tickPresetStage('Detect + OCR', 'recognize', false)(recognize)
    expect(one.presets[0]!.stages).toEqual(['recognize'])

    expect(deletePreset('Detect + OCR')(runs()).presets).toEqual([builtInRuns.presets[1]])
    expect(setRuns({ retries: 4, order: 'stages' })(runs())).toEqual({
      ...builtInRuns,
      retries: 4,
      order: 'stages',
    })
  })

  it('takes retries from 0 to 5, whole numbers only', () => {
    expect([0, 2, 5].every(isRetries)).toBe(true)
    expect([null, -1, 6, 2.5, Number.NaN].some(isRetries)).toBe(false)
  })
})
