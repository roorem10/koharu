import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import { playFinishSound } from '@/components/booharu/runs/sound'

/** The notes the chime started, by frequency. */
const played: number[] = []
/** Each start the chime asked of the context, waiting to be let through. */
const starts: Array<() => void> = []

/** Web Audio as a WebView gives it before a click lets it play: suspended, starting only once the
 * WebView lets a `resume()` through. */
class SuspendedAudioContext {
  currentTime = 0
  state = 'suspended'
  destination = {}
  resume() {
    return new Promise<void>((resolve) => starts.push(resolve))
  }
  createOscillator() {
    const frequency = { value: 0 }
    return {
      frequency,
      connect: vi.fn(),
      disconnect: vi.fn(),
      stop: vi.fn(),
      start: () => played.push(frequency.value),
      onended: null,
    }
  }
  createGain() {
    return {
      gain: { setValueAtTime: vi.fn(), exponentialRampToValueAtTime: vi.fn() },
      connect: vi.fn(),
      disconnect: vi.fn(),
    }
  }
}

/** Lets every start asked so far through, and what follows it run. */
async function letStart() {
  for (const start of starts.splice(0)) start()
  for (let turn = 0; turn < 5; turn += 1) await Promise.resolve()
}

describe('Booharu finish sound on a suspended context', () => {
  beforeAll(() => {
    vi.stubGlobal('AudioContext', SuspendedAudioContext)
  })
  afterEach(() => {
    played.length = 0
    starts.length = 0
    vi.useRealTimers()
  })

  it('plays once the context starts in time', async () => {
    playFinishSound(false)
    expect(played).toEqual([])
    await letStart()
    expect(played).toEqual([660, 880])
  })

  it('plays nothing when the context starts too late, as on a click long after the run', async () => {
    vi.useFakeTimers()
    playFinishSound(true)
    vi.advanceTimersByTime(60_000)
    await letStart()
    expect(played).toEqual([])
  })
})
