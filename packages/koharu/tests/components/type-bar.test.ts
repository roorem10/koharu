import { describe, expect, it } from 'vitest'

import type { CharStyle } from '@koharu/ipc/protocol'

import { sharedChars, swappedColors } from '@/components/editor/TypeBar'

describe('Booharu swap of text and border colours', () => {
  it('swaps a visible border with the text colour', () => {
    expect(swappedColors({ fill: [0, 0, 0, 1], stroke: [1, 1, 1, 1], stroke_width: 4 })).toEqual({
      fill: [1, 1, 1, 1],
      stroke: [0, 0, 0, 1],
    })
  })

  it('borders text without one in its colour, over a fill that stands out', () => {
    // Dark red text gets a white fill; yellow text, transparent border or not, a black one.
    expect(swappedColors({ fill: [0.5, 0, 0, 1] })).toEqual({
      fill: [1, 1, 1, 1],
      stroke: [0.5, 0, 0, 1],
      stroke_width: 1.5,
    })
    expect(swappedColors({ fill: [1, 1, 0, 1], stroke: [0, 0, 0, 0], stroke_width: 6 })).toEqual({
      fill: [0, 0, 0, 1],
      stroke: [1, 1, 0, 1],
      stroke_width: 6,
    })
  })
})

describe('Booharu second border', () => {
  it('can go on texts whose own borders differ, not on a text without one', () => {
    const red: CharStyle = { stroke: [1, 0, 0, 1], stroke_width: 2 }
    const white: CharStyle = { stroke: [1, 1, 1, 1], stroke_width: 5 }
    const bordered = sharedChars([
      { content: [{ insert: 'Boom' }], style: { chars: { ...red } } },
      { content: [{ insert: 'Bang' }], style: { chars: { ...white } } },
    ])
    expect(bordered.mixed).toEqual(['stroke', 'stroke_width'])
    expect(bordered.bordered).toBe(true)
    const one = sharedChars([
      { content: [{ insert: 'Boom' }], style: { chars: { ...red } } },
      { content: [{ insert: 'quiet' }] },
    ])
    expect(one.bordered).toBe(false)
  })
})
