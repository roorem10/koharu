'use client'

import type { Job } from '@koharu/bridge/protocol'

let context: AudioContext | null = null

export function prepareFinishSound() {
  try {
    if (!globalThis.AudioContext) return
    context ??= new AudioContext()
    if (context.state === 'suspended') void context.resume().catch(() => undefined)
  } catch {
    // Audio is optional, including when the WebView denies audio playback.
  }
}

export function playFinishSound(problem: boolean) {
  try {
    prepareFinishSound()
    if (!context) return
    const now = context.currentTime
    for (const [index, frequency] of (problem ? [330, 247] : [660, 880]).entries()) {
      const oscillator = context.createOscillator()
      const gain = context.createGain()
      const start = now + index * 0.16
      oscillator.frequency.value = frequency
      gain.gain.setValueAtTime(0.0001, start)
      gain.gain.exponentialRampToValueAtTime(problem ? 0.025 : 0.055, start + 0.015)
      gain.gain.exponentialRampToValueAtTime(0.0001, start + 0.14)
      oscillator.connect(gain)
      gain.connect(context.destination)
      oscillator.start(start)
      oscillator.stop(start + 0.15)
      oscillator.onended = () => {
        oscillator.disconnect()
        gain.disconnect()
      }
    }
  } catch {
    // A sound failure must never affect processing or job updates.
  }
}

export function soundForJobTransition(previous: Job | undefined, job: Job) {
  if (previous?.state !== 'running' || !job.finish_sound) return
  if (
    job.state === 'finished' ||
    job.state === 'finished_with_problems' ||
    job.state === 'failed'
  ) {
    playFinishSound(job.state !== 'finished')
  }
}
