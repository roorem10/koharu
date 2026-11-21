import { describe, expect, it, vi } from 'vitest'

import { borderCompanion, paragraphShare, setEach } from '@/lib/booharu/mixed'
import { setStyle } from '@/lib/scene'
import { hexToRgba } from '@/lib/store'
import {
  commands,
  type CharStyle,
  type Edit,
  type Gradient,
  type Layer,
  type Text,
} from '@koharu/ipc/protocol'

import { block } from './boxes'

/** A text layer `id` lettered as `fields` say. */
function text(id: string, fields: Partial<Text> = {}): Layer {
  return {
    id,
    name: id,
    visible: true,
    locked: false,
    opacity: 1,
    blend: 'normal',
    attributes: {},
    kind: 'text',
    content: [{ insert: 'Hello' }],
    ...fields,
  } as Layer
}

/** A block whose translation is lettered as `fields` say. */
function translated(id: string, fields: Partial<Text> = {}): Layer {
  return block(id, 'こんにちは', { translation: { content: [{ insert: 'Hi' }], ...fields } })
}

const box = (fit?: 'shrink' | 'none' | 'grow' | 'best') => ({
  box: { bounds: { x: 0, y: 0, width: 100, height: 50 }, ...(fit ? { fit } : {}) },
})

/** The edits sent while `run` runs. */
async function editsOf(run: () => Promise<unknown>): Promise<Edit[]> {
  const edits: Edit[] = []
  vi.spyOn(commands, 'edit').mockImplementation(async (edit) => {
    edits.push(edit)
    return null as never
  })
  await run()
  return edits
}

describe('Booharu paragraph values the selected texts share (S11-h)', () => {
  it('tells each paragraph value the texts share, as each is drawn', () => {
    const share = paragraphShare([
      text('one', { style: { align: 'center', line_height: 1.2, stroke_method: 'expansion' } }),
      // Unset, each is the same as drawn: centred, 1.2 and expansion, Koharu's border.
      translated('two', { frame: box('shrink') }),
      text('three', { frame: 'point' }),
    ])

    expect(share).toEqual({
      align: { mixed: false, value: 'center' },
      vertical: { mixed: false, value: false },
      // A box that shrinks its text, and point text, show the size set alike.
      fit: { mixed: false, value: false },
      line_height: { mixed: false, value: 1.2 },
      stroke_method: { mixed: false, value: 'expansion' },
    })
  })

  it('tells each value that differs', () => {
    const share = paragraphShare([
      text('one', {
        direction: 'vertical',
        frame: box('best'),
        style: { line_height: 1.5, stroke_method: 'contour' },
      }),
      translated('two', { frame: box() }),
    ])

    expect(share).toEqual({
      // Unset, a vertical text starts its lines and a horizontal one centres them.
      align: { mixed: true },
      vertical: { mixed: true },
      fit: { mixed: true },
      line_height: { mixed: true },
      stroke_method: { mixed: true },
    })
  })

  it('reads a block by its translation, and leaves out other layers', () => {
    const share = paragraphShare([
      translated('one', { style: { align: 'end' } }),
      text('two', { style: { align: 'end' } }),
      { ...text('raster'), kind: 'raster' } as Layer,
      block('untranslated', 'こんにちは'),
    ])

    expect(share.align).toEqual({ mixed: false, value: 'end' })
  })

  it('shares a new text’s values with no target', () => {
    expect(paragraphShare([])).toEqual({
      align: { mixed: false, value: 'center' },
      vertical: { mixed: false, value: false },
      fit: { mixed: false, value: false },
      line_height: { mixed: false, value: 1.2 },
      stroke_method: { mixed: false, value: 'expansion' },
    })
  })
})

describe('Booharu style edit of each target’s own (S11-h)', () => {
  const marked = text('marked', {
    content: [
      { insert: 'Hel', attributes: { stroke_width: 4, fill: [1, 0, 0, 1] } },
      { insert: 'lo' },
    ],
  })
  const targets = [text('plain'), marked, translated('block'), block('untranslated', 'やあ')]

  it('writes for one patch what setStyle writes, as one edit', async () => {
    const patch = { chars: { stroke_width: 2 }, align: 'end' as const }

    const each = await editsOf(() => setEach(targets, () => patch))
    const same = await editsOf(() => setStyle(targets, patch))

    expect(each).toHaveLength(1)
    expect(each).toEqual(same)
    // The marked text's own widths go, its other marks stay.
    expect(each[0]!.ops).toContainEqual({
      op: 'set_rich_text',
      layer: 'marked',
      spans: [
        { insert: 'Hel', attributes: { fill: [1, 0, 0, 1] } },
        { insert: 'lo', attributes: {} },
      ],
    })
  })

  it('gives each target its own patch, and none to a target given none', async () => {
    const edits = await editsOf(() =>
      setEach(targets, (layer) =>
        layer.id === 'plain' ? null : { chars: { stroke: hexToRgba('#FF0000') } },
      ),
    )

    expect(edits).toEqual([
      {
        label: 'set_style',
        ops: [
          // No width is set, so the marked text keeps its widths.
          {
            op: 'update_layer',
            layer: 'marked',
            patch: { style: { chars: { stroke: [1, 0, 0, 1] } } },
          },
          {
            op: 'update_layer',
            layer: 'block',
            patch: { translation: { style: { chars: { stroke: [1, 0, 0, 1] } } } },
          },
          {
            op: 'update_layer',
            layer: 'untranslated',
            patch: { translation: { style: { chars: { stroke: [1, 0, 0, 1] } } } },
          },
        ],
      },
    ])
  })
})

describe('Booharu border value going with the one set (S11-h)', () => {
  it('gives the default only where a text has no value of its own', () => {
    const none = text('none')
    const own = text('own', {
      style: { chars: { stroke: [0, 0, 1, 1], stroke_width: 2, outer_stroke_width: 5 } },
    })
    const zero = text('zero', { style: { chars: { stroke_width: 0, outer_stroke_width: 0 } } })

    expect(borderCompanion(none, 'stroke_width')).toEqual({ stroke_width: 1.5 })
    expect(borderCompanion(none, 'stroke')).toEqual({ stroke: [1, 1, 1, 1] })
    expect(borderCompanion(none, 'outer_stroke_width')).toEqual({ outer_stroke_width: 3 })
    expect(borderCompanion(none, 'outer_stroke')).toEqual({ outer_stroke: [0, 0, 0, 1] })
    expect(borderCompanion(own, 'stroke_width')).toEqual({})
    expect(borderCompanion(own, 'stroke')).toEqual({})
    expect(borderCompanion(own, 'outer_stroke_width')).toEqual({})
    // No width is no border.
    expect(borderCompanion(zero, 'stroke_width')).toEqual({ stroke_width: 1.5 })
    expect(borderCompanion(zero, 'outer_stroke_width')).toEqual({ outer_stroke_width: 3 })
  })

  it('reads the characters’ own marks over the text’s style', () => {
    const allMarked = text('all', {
      content: [
        { insert: 'Hel', attributes: { stroke_width: 2 } },
        { insert: 'lo', attributes: { stroke_width: 2 } },
      ],
    })
    const someMarked = text('some', {
      style: { chars: { stroke: [0, 0, 0, 1] } },
      content: [{ insert: 'Hel', attributes: { stroke: [1, 0, 0, 1] } }, { insert: 'lo' }],
    })
    const someWide = text('wide', {
      content: [{ insert: 'Hel', attributes: { stroke_width: 2 } }, { insert: 'lo' }],
    })

    expect(borderCompanion(allMarked, 'stroke_width')).toEqual({})
    // Characters that differ keep their own.
    expect(borderCompanion(someMarked, 'stroke')).toEqual({})
    expect(borderCompanion(someWide, 'stroke_width')).toEqual({})
    // A block reads its translation; one with none has no value of its own.
    const wideBlock = translated('block', { style: { chars: { stroke_width: 3 } } })
    expect(borderCompanion(wideBlock, 'stroke_width')).toEqual({})
    expect(borderCompanion(block('untranslated', 'やあ'), 'stroke_width')).toEqual({
      stroke_width: 1.5,
    })
  })
})

describe('Booharu fill and gradient fill, one paint, in style edits (S12-c)', () => {
  const gradient: Gradient = {
    kind: 'linear',
    angle: 90,
    stops: [
      { offset: 0, color: [1, 0, 0, 1] },
      { offset: 1, color: [0, 0, 1, 1] },
    ],
  }
  const painted = text('painted', {
    content: [
      { insert: 'Hel', attributes: { fill: [1, 0, 0, 1], weight: 700 } },
      { insert: 'lo', attributes: { fill_gradient: gradient } },
    ],
  })

  it.each<[string, CharStyle]>([
    ['a colour', { fill: [0, 1, 0, 1], fill_gradient: null }],
    ['a colour alone', { fill: [0, 1, 0, 1] }],
    ['a gradient', { fill_gradient: gradient }],
    ['no gradient', { fill_gradient: null }],
  ])(
    'takes both paint keys off the marks for %s, in setStyle and setEach alike',
    async (_, chars) => {
      const style = await editsOf(() => setStyle([painted], { chars }))
      const each = await editsOf(() => setEach([painted], () => ({ chars })))

      expect(each).toEqual(style)
      expect(style).toEqual([
        {
          label: 'set_style',
          ops: [
            { op: 'update_layer', layer: 'painted', patch: { style: { chars } } },
            {
              op: 'set_rich_text',
              layer: 'painted',
              spans: [
                { insert: 'Hel', attributes: { weight: 700 } },
                { insert: 'lo', attributes: {} },
              ],
            },
          ],
        },
      ])
    },
  )

  it('leaves the paint marks for an edit setting neither', async () => {
    const patch = { chars: { tracking: 100, size: 30 } }

    const style = await editsOf(() => setStyle([painted], patch))
    const each = await editsOf(() => setEach([painted], () => patch))

    expect(each).toEqual(style)
    expect(style).toEqual([
      {
        label: 'set_style',
        ops: [{ op: 'update_layer', layer: 'painted', patch: { style: patch } }],
      },
    ])
  })
})
