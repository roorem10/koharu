'use client'

// Booharu 0.0.4's finish chime, played with Web Audio: two notes, 660 then 880 Hz, or quieter, 330
// then 247 Hz, when the run had problems. When a run chimes is lib/booharu/chime.ts's to say.

let context: AudioContext | null = null

/** How long a context the WebView holds suspended may take to start for a chime: one that starts
 * later, as when a click lets it long after the run ended, plays nothing then. */
const START_WAIT = 1000

/** Readies the chime's audio on a click, as a WebView starts sound only after a user gesture: the
 * Run button's, a preset's, and Retry failed pages'. */
export function prepareFinishSound() {
  try {
    if (!globalThis.AudioContext) return
    context ??= new AudioContext()
    if (context.state === 'suspended') void context.resume().catch(() => undefined)
  } catch {
    // Audio is optional, including when the WebView denies audio playback.
  }
}

/** Plays the chime, the lower one when `problem`. Audio the WebView won't start, as for a run the
 * agent started before any click readied it, stays silent. */
export function playFinishSound(problem: boolean) {
  try {
    prepareFinishSound()
    const audio = context
    if (!audio) return
    if (audio.state === 'running') {
      notes(audio, problem)
      return
    }
    const asked = Date.now()
    void audio
      .resume()
      .then(() => {
        if (Date.now() - asked < START_WAIT) notes(audio, problem)
      })
      .catch(() => undefined)
  } catch {
    // A sound failure must never affect processing or job updates.
  }
}

function notes(audio: AudioContext, problem: boolean) {
  const now = audio.currentTime
  for (const [index, frequency] of (problem ? [330, 247] : [660, 880]).entries()) {
    const oscillator = audio.createOscillator()
    const gain = audio.createGain()
    const start = now + index * 0.16
    oscillator.frequency.value = frequency
    gain.gain.setValueAtTime(0.0001, start)
    gain.gain.exponentialRampToValueAtTime(problem ? 0.025 : 0.055, start + 0.015)
    gain.gain.exponentialRampToValueAtTime(0.0001, start + 0.14)
    oscillator.connect(gain)
    gain.connect(audio.destination)
    oscillator.start(start)
    oscillator.stop(start + 0.15)
    oscillator.onended = () => {
      oscillator.disconnect()
      gain.disconnect()
    }
  }
}
