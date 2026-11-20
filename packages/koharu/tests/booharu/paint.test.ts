import { describe, expect, it } from 'vitest'

import {
  BLACK,
  charsOr,
  darker,
  endColor,
  flatColor,
  gradientShown,
  isLight,
  lighter,
  normalAngle,
  ownColor,
  paintUnit,
  seedGradient,
  sharedColor,
  sharedPaint,
  spanStyles,
  withEndColor,
} from '@/lib/booharu/paint'
import type { CharStyle, Gradient, Rgba, Text } from '@koharu/ipc/protocol'

const red: Rgba = [1, 0, 0, 1]
const blue: Rgba = [0, 0, 1, 1]

/** A linear gradient from red to blue at `angle`. */
function gradient(angle: number): Gradient {
  return {
    kind: 'linear',
    angle,
    stops: [
      { offset: 0, color: red },
      { offset: 1, color: blue },
    ],
  }
}

/** A text styled `chars` whose content is `content`. */
function lettered(chars: CharStyle, content: Text['content'] = [{ insert: 'Hello' }]): Text {
  return { content, style: { chars } } as Text
}

describe('Booharu paint, as CharStyle::or merges it (S12-c)', () => {
  const boxed: CharStyle = { fill: blue, fill_gradient: gradient(90), tracking: 100 }

  it('lets a colour on a word beat the gradient the box gives it', () => {
    const word = charsOr({ fill: red }, boxed)
    expect([word.fill, word.fill_gradient]).toEqual([red, undefined])
    expect('fill_gradient' in word).toBe(false)
  })

  it("lets a word's own gradient beat the box's colour and gradient alike", () => {
    const own: CharStyle = { fill_gradient: gradient(0) }
    expect(charsOr(own, { fill: blue })).toEqual({ fill: blue, fill_gradient: gradient(0) })
    expect(charsOr(own, boxed).fill_gradient).toEqual(gradient(0))
  })

  it("gives a word with neither the box's paint and letter spacing", () => {
    expect(charsOr({}, boxed)).toEqual(boxed)
  })

  it('keeps a gradient beside a colour, which so wins', () => {
    const both: CharStyle = { fill: red, fill_gradient: gradient(0) }
    expect(charsOr(both, boxed)).toEqual({ fill: red, fill_gradient: gradient(0), tracking: 100 })
  })

  it("lets a word's letter spacing beat the box's, and reads a null mark as none", () => {
    expect(charsOr({ tracking: -50 }, boxed).tracking).toBe(-50)
    expect(charsOr({ fill: null, fill_gradient: null, tracking: null }, boxed)).toEqual(boxed)
  })

  it("styles each span of a text by its marks over the text's style", () => {
    const text = lettered({ fill: blue, fill_gradient: gradient(90) }, [
      { insert: 'Hello ' },
      { insert: 'world', attributes: { fill: red } },
    ])
    expect(spanStyles(text)).toEqual([{ fill: blue, fill_gradient: gradient(90) }, { fill: red }])
    expect(spanStyles(lettered({ size: 30 }, []))).toEqual([{ size: 30 }])
  })
})

describe('Booharu paint that characters show (S12-c)', () => {
  it('shows the shared gradient, or the flat colour, black where none is set', () => {
    expect(sharedPaint({ fill: red, fill_gradient: gradient(90) }, [])).toEqual({
      gradient: gradient(90),
    })
    expect(sharedPaint({ fill: red }, new Set())).toEqual({ color: red })
    expect(sharedPaint({}, [])).toEqual({ color: BLACK })
  })

  it('shows none while some show a gradient, or the colours differ outside one', () => {
    expect(sharedPaint({ fill: red }, ['fill_gradient'])).toBeNull()
    expect(sharedPaint({}, new Set(['fill']))).toBeNull()
    // The colours under one gradient don't show.
    expect(sharedPaint({ fill_gradient: gradient(90) }, ['fill'])).toEqual({
      gradient: gradient(90),
    })
  })

  it('tells every character showing a gradient from some showing one', () => {
    expect(gradientShown({ fill_gradient: gradient(90) }, [], null)).toBe('all')
    expect(gradientShown({ fill: red }, [], null)).toBe('none')
    const styles = [{ fill_gradient: gradient(90) }, { fill_gradient: gradient(0) }]
    expect(gradientShown({}, ['fill_gradient'], styles)).toBe('all')
    expect(gradientShown({}, ['fill_gradient'], [...styles, { fill: red }])).toBe('some')
    // While typing, the canvas doesn't say which.
    expect(gradientShown({}, ['fill_gradient'], null)).toBe('some')
  })

  it('takes the flat colour from the fill, under any gradient', () => {
    expect(flatColor({ fill: red, fill_gradient: gradient(90) })).toEqual(red)
    expect(flatColor({})).toEqual(BLACK)
    expect(sharedColor({ fill: red }, [], { fill: blue })).toEqual(red)
    expect(sharedColor({}, ['fill'], { fill: blue })).toEqual(blue)
    expect(sharedColor({}, ['fill'])).toEqual(BLACK)
  })

  it("takes a text's own colour from the fill every character has, or its style's", () => {
    expect(ownColor(lettered({ fill: blue }))).toEqual(blue)
    expect(ownColor(lettered({}, [{ insert: 'Hi', attributes: { fill: red } }]))).toEqual(red)
    const words = lettered({ fill: blue }, [
      { insert: 'Hello ' },
      { insert: 'world', attributes: { fill: red } },
    ])
    expect(ownColor(words)).toEqual(blue)
    expect(
      ownColor(lettered({}, [{ insert: 'a' }, { insert: 'b', attributes: { fill: red } }])),
    ).toEqual(BLACK)
  })
})

describe('Booharu gradient seed and ends (S12-c)', () => {
  it('seeds a linear gradient at 90° from a colour to one 60% lighter', () => {
    expect(seedGradient(red)).toEqual({
      kind: 'linear',
      angle: 90,
      stops: [
        { offset: 0, color: red },
        { offset: 1, color: [1, 153 / 255, 153 / 255, 1] },
      ],
    })
    expect(seedGradient(BLACK).stops[1]?.color).toEqual([153 / 255, 153 / 255, 153 / 255, 1])
  })

  it('seeds one 60% darker from a light colour, keeping its alpha', () => {
    const light: Rgba = [1, 1, 1, 0.5]
    expect(isLight(light)).toBe(true)
    expect(seedGradient(light).stops).toEqual([
      { offset: 0, color: light },
      { offset: 1, color: [102 / 255, 102 / 255, 102 / 255, 0.5] },
    ])
  })

  it('shades each channel to a 255th, within 0 and 1', () => {
    expect(lighter([0.5, 0, 1, 1], 0.5)).toEqual([191 / 255, 128 / 255, 1, 1])
    expect(darker([0.5, 0, 1, 0.25], 0.5)).toEqual([64 / 255, 0, 128 / 255, 0.25])
    expect(isLight([0, 0, 1, 1])).toBe(false)
  })

  it('reads and colours the first and last stops, each keeping its alpha', () => {
    const three: Gradient = {
      kind: 'radial',
      angle: 0,
      stops: [
        { offset: 0, color: [1, 0, 0, 0.5] },
        { offset: 0.5, color: [0, 1, 0, 1] },
        { offset: 1, color: [0, 0, 1, 0.25] },
      ],
    }
    expect(endColor(three, 'start')).toEqual([1, 0, 0, 0.5])
    expect(endColor(three, 'end')).toEqual([0, 0, 1, 0.25])

    const started = withEndColor(three, 'start', [0, 0, 0, 1])
    expect(started.stops.map((stop) => stop.color)).toEqual([
      [0, 0, 0, 0.5],
      [0, 1, 0, 1],
      [0, 0, 1, 0.25],
    ])
    const ended = withEndColor(three, 'end', [1, 1, 1, 1])
    expect(ended.stops[2]).toEqual({ offset: 1, color: [1, 1, 1, 0.25] })
    expect(ended.kind).toBe('radial')
    // The gradient it was made from stays as it was.
    expect(three.stops[0]?.color).toEqual([1, 0, 0, 0.5])
  })

  it('shows any angle as the same direction from -180 to 180', () => {
    expect([0, 90, 180, -180, 270, -190, 540, 360].map(normalAngle)).toEqual([
      0, 90, 180, -180, -90, 170, 180, 0,
    ])
  })
})

describe('Booharu paint unit in style edits (S12-c)', () => {
  it('adds the other paint key wherever one is set, and nothing otherwise', () => {
    expect(paintUnit(['fill'])).toEqual(['fill', 'fill_gradient'])
    expect(paintUnit(['size', 'fill_gradient'])).toEqual(['size', 'fill_gradient', 'fill'])
    expect(paintUnit(['fill', 'fill_gradient'])).toEqual(['fill', 'fill_gradient'])
    expect(paintUnit(['size', 'tracking'])).toEqual(['size', 'tracking'])
  })
})
