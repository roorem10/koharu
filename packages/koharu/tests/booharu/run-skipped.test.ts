import { describe, expect, it, vi } from 'vitest'

import { receiveRuns, useKoharuStore } from '@/lib/store'
import { toast } from '@koharu/ui/components/toast'

import { done, openChapter, queued, skipped, tell } from './jobs'

const job = (run: number) => useKoharuStore.getState().jobs[String(run)]

const hint =
  'To translate one anyway, pick another category for it, or change the Sound effects setting.'

describe('What a run left alone', () => {
  it('keeps what each page stage left alone, a stage told again replacing what it told', () => {
    openChapter('Chapter 1')
    tell(
      queued(1),
      skipped(1, 'p1', 'recognize', { hand_fixed: 2, locked: 1 }),
      skipped(1, 'p2', 'recognize', { locked: 1 }),
      skipped(1, 'p1', 'translate', { sound_effects: 3, locked: 1 }),
      // A page read again after a failed try tells what it left again.
      skipped(1, 'p1', 'recognize', { hand_fixed: 1, locked: 1 }),
    )
    expect(job(1)?.booharu).toMatchObject({
      skipped: {
        recognize: {
          p1: { sound_effects: 0, hand_fixed: 1, locked: 1 },
          p2: { sound_effects: 0, hand_fixed: 0, locked: 1 },
        },
        translate: { p1: { sound_effects: 3, hand_fixed: 0, locked: 1 } },
      },
      namedBlocks: false,
    })

    // A run on blocks says so with what its stages left.
    tell(queued(2, ['translate'], false), skipped(2, 'p1', 'translate', { sound_effects: 1 }, true))
    expect(job(2)?.booharu?.namedBlocks).toBe(true)
  })

  it('says once, as the run ends, what it left alone, each locked box counted once', () => {
    openChapter('Chapter 1')
    const shown = vi.spyOn(toast, 'add')
    tell(
      queued(1),
      skipped(1, 'p1', 'detect', { hand_fixed: 3, locked: 2 }),
      skipped(1, 'p1', 'recognize', { hand_fixed: 1, locked: 2 }),
      skipped(1, 'p1', 'translate', { hand_fixed: 2, locked: 1, sound_effects: 4 }),
      // Cleaning also leaves the block whose clean-up alone is locked.
      skipped(1, 'p1', 'inpaint', { locked: 3 }),
      skipped(1, 'p2', 'recognize', { locked: 1 }),
      skipped(1, 'p2', 'translate', { locked: 1 }),
    )
    expect(shown).not.toHaveBeenCalled()

    // A run over pages keeps sound effects as art on every page, as the setting says, and says
    // nothing of them.
    tell(done(1))
    expect(shown).toHaveBeenCalledOnce()
    expect(shown).toHaveBeenCalledWith({
      type: 'info',
      title: 'Detection kept 3 boxes made or changed by hand.',
      description:
        "1 box fixed by hand wasn't read again. 2 boxes fixed by hand weren't translated again. " +
        '4 locked boxes were left as they are.',
    })
  })

  it('words what Detect again alone left as kept', () => {
    openChapter('Chapter 1')
    const shown = vi.spyOn(toast, 'add')
    tell(queued(1, ['detect']), skipped(1, 'p1', 'detect', { hand_fixed: 1, locked: 2 }), done(1))
    expect(shown).toHaveBeenCalledWith({
      type: 'info',
      title: 'Detection kept 1 box made or changed by hand.',
      description: 'Detection kept 2 locked boxes.',
    })
  })

  it('counts the sound effects a run naming blocks kept as art, and says how to translate one', () => {
    openChapter('Chapter 1')
    const shown = vi.spyOn(toast, 'add')
    // A box's Translate on an effect kept as art.
    tell(
      queued(1, ['translate'], false),
      skipped(1, 'p1', 'translate', { sound_effects: 1 }, true),
      done(1),
    )
    expect(shown).toHaveBeenLastCalledWith({
      type: 'info',
      title: '1 sound effect was kept as art.',
      description: hint,
    })

    tell(
      queued(2, ['translate'], false),
      skipped(2, 'p1', 'translate', { sound_effects: 2, locked: 1 }, true),
      { kind: 'ended', run: 2, outcome: { kind: 'stopped' } },
    )
    expect(shown).toHaveBeenLastCalledWith({
      type: 'info',
      title: '1 locked box was left as it is.',
      description: `2 sound effects were kept as art. ${hint}`,
    })
  })

  it('says nothing of a run that left nothing alone, nor of one told again as the editor loads', () => {
    openChapter('Chapter 1')
    const shown = vi.spyOn(toast, 'add')
    tell(queued(1), done(1))
    receiveRuns([queued(2), skipped(2, 'p1', 'recognize', { locked: 1 }), done(2)])
    expect(shown).not.toHaveBeenCalled()
    // Its record keeps what was left all the same.
    expect(job(2)?.booharu?.skipped).toEqual({
      recognize: { p1: { sound_effects: 0, hand_fixed: 0, locked: 1 } },
    })
  })
})
