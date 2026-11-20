import { describe, expect, it } from 'vitest'

import { sharedChars, swappedColors } from '@/components/editor/TypeBar'
import type { CharStyle } from '@koharu/ipc/protocol'

describe('Booharu swap of text and border colours', () => {
  it('swaps a visible border with the text colour', () => {
    expect(swappedColors({ fill: [0, 0, 0, 1], stroke: [1, 1, 1, 1], stroke_width: 4 })).toEqual({
      fill: [1, 1, 1, 1],
      fill_gradient: null,
      stroke: [0, 0, 0, 1],
    })
  })

  it('borders text without one in its colour, over a fill that stands out', () => {
    // Dark red text gets a white fill; yellow text, transparent border or not, a black one.
    expect(swappedColors({ fill: [0.5, 0, 0, 1] })).toEqual({
      fill: [1, 1, 1, 1],
      fill_gradient: null,
      stroke: [0.5, 0, 0, 1],
      stroke_width: 1.5,
    })
    expect(swappedColors({ fill: [1, 1, 0, 1], stroke: [0, 0, 0, 0], stroke_width: 6 })).toEqual({
      fill: [0, 0, 0, 1],
      fill_gradient: null,
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

describe('Booharu paint the characters share (S12-c)', () => {
  const gradient: CharStyle['fill_gradient'] = {
    kind: 'linear',
    angle: 90,
    stops: [
      { offset: 0, color: [1, 0, 0, 1] },
      { offset: 1, color: [0, 0, 1, 1] },
    ],
  }

  it('shares the gradient of a gradient box, and its letter spacing', () => {
    const shared = sharedChars([
      {
        content: [{ insert: 'Boom' }],
        style: { chars: { fill: [0, 0, 0, 1], fill_gradient: gradient, tracking: 50 } },
      },
    ])
    expect(shared).toMatchObject({
      chars: { fill: [0, 0, 0, 1], fill_gradient: gradient, tracking: 50 },
      mixed: [],
    })
  })

  it('reads a word given a colour in a gradient box as showing that colour alone', () => {
    const shared = sharedChars([
      {
        content: [{ insert: 'Boom ' }, { insert: 'bang', attributes: { fill: [1, 0, 0, 1] } }],
        style: { chars: { fill: [0, 0, 0, 1], fill_gradient: gradient } },
      },
    ])
    expect(shared.mixed).toEqual(['fill', 'fill_gradient'])
    expect(shared.chars).toEqual({})

    const word = sharedChars([
      {
        content: [{ insert: 'bang', attributes: { fill: [1, 0, 0, 1] } }],
        style: { chars: { fill_gradient: gradient } },
      },
    ])
    expect(word).toMatchObject({ chars: { fill: [1, 0, 0, 1] }, mixed: [] })
    expect('fill_gradient' in word.chars).toBe(false)
  })

  it('tells the gradients and the letter spacings that differ', () => {
    const shared = sharedChars([
      { content: [{ insert: 'Boom' }], style: { chars: { fill_gradient: gradient } } },
      {
        content: [{ insert: 'Bang', attributes: { tracking: 200 } }],
        style: { chars: { fill_gradient: { ...gradient!, angle: 0 } } },
      },
    ])
    expect(shared.mixed).toEqual(['fill_gradient', 'tracking'])
    // A colour differing under a gradient is no mixed paint.
    const fills = sharedChars([
      {
        content: [
          { insert: 'Boom ' },
          { insert: 'bang', attributes: { fill: [1, 0, 0, 1], fill_gradient: gradient } },
        ],
        style: { chars: { fill_gradient: gradient } },
      },
    ])
    expect(fills.mixed).toEqual(['fill'])
    expect(fills.chars.fill_gradient).toEqual(gradient)
  })
})
