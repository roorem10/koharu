import { renderHook } from '@testing-library/react'
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { useBooharuRunGroups } from '@/lib/booharu/chime'
import { events, type BooharuRunGroup, type Outcome } from '@koharu/ipc/protocol'

import { done, every, openChapter, queued, tell } from './jobs'
import { settingsDesktop } from './settings'

/** The notes the chime started, by frequency, and the loudest each rose to. */
const played: Array<{ frequency: number; peak: number }> = []

/** Web Audio as the WebView gives it once a click let it start. */
class AudioContextMock {
  currentTime = 0
  state = 'running'
  destination = {}
  createOscillator() {
    const frequency = { value: 0 }
    return {
      frequency,
      connect: vi.fn(),
      disconnect: vi.fn(),
      stop: vi.fn(),
      start: () => played.push({ frequency: frequency.value, peak: 0 }),
      onended: null,
    }
  }
  createGain() {
    return {
      gain: {
        setValueAtTime: vi.fn(),
        exponentialRampToValueAtTime: (value: number) => {
          const note = played.at(-1)
          if (note && value > note.peak) note.peak = value
        },
      },
      connect: vi.fn(),
      disconnect: vi.fn(),
    }
  }
}

/** What the chime played: `high` for done, `low` for problems, by its two notes. */
function heard(): string[] {
  const tones: string[] = []
  for (let at = 0; at < played.length; at += 2) {
    const pair = played.slice(at, at + 2).map((note) => note.frequency)
    tones.push(pair.join() === '660,880' ? 'high' : pair.join() === '330,247' ? 'low' : '?')
  }
  return tones
}

/** Lets a chime that would come, come: it waits for the finish sound's setting. */
const settled = () => new Promise((resolve) => setTimeout(resolve, 0))

const ended = (run: number, outcome: Outcome) => ({ kind: 'ended' as const, run, outcome })
const failure = { page: 'p2', stage: 'translate' as const, reason: 'no connection' }

/** Tells each run a carrying-on group queued, as booharu_run_group does. */
function groupTeller() {
  let handler: ((group: BooharuRunGroup) => void) | undefined
  vi.spyOn(events, 'booharuRunGroup').mockImplementation(async (listen) => {
    handler = listen
    return () => undefined
  })
  renderHook(() => useBooharuRunGroups())
  return (group: BooharuRunGroup) => handler?.(group)
}

describe('Booharu finish chime', () => {
  beforeAll(() => {
    vi.stubGlobal('AudioContext', AudioContextMock)
  })
  beforeEach(() => {
    played.length = 0
    openChapter('Chapter 1')
  })

  it('chimes as a run over pages ends, lower with problems, and never when it stopped', async () => {
    settingsDesktop()
    tell(queued(1), done(1))
    await settled()
    expect(heard()).toEqual(['high'])
    expect(played[0]?.peak).toBe(0.055)

    tell(queued(2), done(2, [failure]))
    tell(queued(3), ended(3, { kind: 'failed', ...failure }))
    tell(queued(4), ended(4, { kind: 'stopped' }))
    await settled()
    expect(heard()).toEqual(['high', 'low', 'low'])
    expect(played[2]?.peak).toBe(0.025)
  })

  it('stays silent for a run on blocks or an area, and once the finish sound is off', async () => {
    const { file } = settingsDesktop()
    tell(queued(1, ['translate'], false), done(1))
    await settled()
    expect(heard()).toEqual([])

    tell(queued(2, every), done(2))
    await settled()
    expect(heard()).toEqual(['high'])
    // The agent's settings tool turns the sound off: the next ending reads the file again.
    file.preferences['booharu-runs'].finish_sound = false
    tell(queued(3, every), done(3))
    await settled()
    expect(heard()).toEqual(['high'])
  })

  it('chimes once for a carrying-on group, as it ends, lower if any of its runs had problems', async () => {
    settingsDesktop()
    const group = groupTeller()

    group({ group: 1, run: 1, index: 0, count: 3 })
    tell(queued(1, ['translate']), done(1, [failure]))
    group({ group: 1, run: 2, index: 1, count: 3 })
    tell(queued(2, ['inpaint', 'translate']), done(2))
    await settled()
    expect(heard()).toEqual([])
    group({ group: 1, run: 3, index: 2, count: 3 })
    tell(queued(3), done(3))
    await settled()
    expect(heard()).toEqual(['low'])

    // A run that fails ends its group, which chimes then; one that stops ends it silent.
    group({ group: 2, run: 4, index: 0, count: 3 })
    tell(queued(4), done(4))
    group({ group: 2, run: 5, index: 1, count: 3 })
    tell(queued(5), ended(5, { kind: 'failed', ...failure }))
    group({ group: 3, run: 6, index: 0, count: 2 })
    tell(queued(6), ended(6, { kind: 'stopped' }))
    group({ group: 4, run: 7, index: 0, count: 1 })
    tell(queued(7), done(7))
    await settled()
    expect(heard()).toEqual(['low', 'low', 'high'])
  })
})
