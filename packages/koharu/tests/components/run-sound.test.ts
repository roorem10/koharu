import { expect, it, vi } from 'vitest'

import { soundForJobTransition } from '@/components/runs/sound'
import type { Job } from '@koharu/bridge/protocol'
function job(overrides: Partial<Job> = {}): Job {
  return {
    id: 'job',
    state: 'running',
    completed: 0,
    total: 2,
    page: null,
    stage: null,
    model: null,
    error: null,
    retry: null,
    failures: [],
    project: 'chapter',
    operation: { operation: 'full' },
    finish_sound: true,
    ...overrides,
  }
}

it('plays two notes once on a terminal job and stays silent on stop or disabled sound', () => {
  const start = vi.fn()
  const frequencies: number[] = []
  const ramp = vi.fn()
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
        start: () => {
          frequencies.push(frequency.value)
          start()
        },
        onended: null,
      }
    }
    createGain() {
      return {
        gain: { setValueAtTime: vi.fn(), exponentialRampToValueAtTime: ramp },
        connect: vi.fn(),
        disconnect: vi.fn(),
      }
    }
  }
  vi.stubGlobal('AudioContext', AudioContextMock)
  const running = job()
  soundForJobTransition(running, job({ state: 'finished' }))
  soundForJobTransition(job({ state: 'finished' }), job({ state: 'finished' }))
  soundForJobTransition(running, job({ state: 'stopped' }))
  soundForJobTransition(running, job({ state: 'failed', finish_sound: false }))
  expect(start).toHaveBeenCalledTimes(2)
  expect(frequencies).toEqual([660, 880])
  soundForJobTransition(running, job({ state: 'finished_with_problems' }))
  expect(frequencies.slice(2)).toEqual([330, 247])
  expect(ramp).toHaveBeenCalledWith(0.025, expect.any(Number))
  vi.unstubAllGlobals()
})
