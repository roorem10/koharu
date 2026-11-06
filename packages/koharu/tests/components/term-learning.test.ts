import { describe, expect, it } from 'vitest'

import { detectLearnedTerm } from '@/components/context/term-learning'

const hina = { source: 'ヒナ', target: 'Hina' }

describe('learning a glossary spelling from a hand edit', () => {
  it('finds a source-grounded spelling change while retaining punctuation', () => {
    expect(
      detectLearnedTerm('ヒナ、来て！', 'Hina, come here!', 'Hinna, come here!', [hina]),
    ).toEqual({
      source: 'ヒナ',
      before: 'Hina',
      after: 'Hinna',
    })
  })

  it('handles a multiword term and deduplicates the glossary/character match', () => {
    const term = { source: '魔王', target: 'Demon King' }
    expect(detectLearnedTerm('魔王だ', 'The Demon King!', 'The Devil King!', [term, term])).toEqual(
      {
        source: '魔王',
        before: 'Demon King',
        after: 'Devil King',
      },
    )
  })

  it('supports non-Latin spellings', () => {
    expect(
      detectLearnedTerm('ヒナ', 'Хина пришла.', 'Хинна пришла.', [{ ...hina, target: 'Хина' }])
        ?.after,
    ).toBe('Хинна')
  })

  it.each([
    ['誰？', 'Hina is here.', 'Hinna is here.'],
    ['ヒナ', 'Hinata is here.', 'Hinnata is here.'],
    ['ヒナ', 'Hina is here.', 'Hina was here.'],
    ['ヒナ', 'Hina is here.', 'Hinna was here.'],
    ['ヒナ', 'Hina is here.', 'She is here.'],
    ['ヒナ', 'Hina is here.', 'Hina is here.'],
    ['ヒナ', 'Hina is here.', ' is here.'],
    ['ヒナ', 'Hina is here.', 'Hina? is here.'],
    ['ヒナ', 'Hina is here.', 'Hina dear is here.'],
    ['ヒナ', 'Hina and Hina.', 'Hinna and Hina.'],
  ])(
    'declines absent sources, substrings, rewrites, deletions and ambiguous repeats',
    (source, before, after) => {
      expect(detectLearnedTerm(source, before, after, [hina])).toBeNull()
    },
  )

  it('does not infer which of two sources sharing one target was changed', () => {
    expect(
      detectLearnedTerm('ヒナ、比奈', 'Hina!', 'Hinna!', [
        hina,
        { source: '比奈', target: 'Hina' },
      ]),
    ).toBeNull()
  })

  it('requires word boundaries for Latin source text', () => {
    expect(detectLearnedTerm('Rind', 'Rin!', 'Ren!', [{ source: 'Rin', target: 'Rin' }])).toBeNull()
  })
})
