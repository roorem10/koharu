import { fireEvent, screen, waitFor } from '@testing-library/react'
import type userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { commands, type CharStyle } from '@koharu/ipc/protocol'

import { block } from './boxes'
import { renderBar, restyled, settled, styled, text } from './typeBar'

const WHITE: CharStyle['stroke'] = [1, 1, 1, 1]
const BLACK: CharStyle['stroke'] = [0, 0, 0, 1]
const RED: CharStyle['stroke'] = [1, 0, 0, 1]

const box = (fit?: 'shrink' | 'best') => ({
  box: { bounds: { x: 0, y: 0, width: 100, height: 50 }, ...(fit ? { fit } : {}) },
})

type User = ReturnType<typeof userEvent.setup>

/** Opens More border options, beside Koharu's border colour and width: the 2nd border, the colour
 * swap and the outline method. */
async function openExtras(user: User) {
  await user.click(screen.getByRole('button', { name: 'More border options' }))
}

/** Opens the colour picker of the well named `well`, Koharu's border colour unless said. */
async function openWell(user: User, well = 'Border color') {
  await user.click(await screen.findByRole('button', { name: well }))
}

/** Types `width` into Koharu's border width, which takes it on Enter, as the field is left. */
async function typeWidth(user: User, width: string) {
  const field = screen.getByRole('textbox', { name: 'Border width' })
  await user.clear(field)
  await user.type(field, `${width}{Enter}`)
}

/** Types `hex` into the open colour picker, which picks it. */
function pickColor(hex: string) {
  fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
    target: { value: hex },
  })
}

describe('Booharu type bar over texts whose paragraphs differ (S11-h)', () => {
  it('presses no alignment while they differ, and a click writes the alignment alone to all', async () => {
    const { edits, bar, user } = await renderBar(
      [text('one', { style: { align: 'start' } }), text('two', { style: { align: 'end' } })],
      { pending: true },
    )

    const buttons = ['Align left', 'Align center', 'Align right'].map((name) =>
      bar.getByRole('button', { name }),
    )
    for (const button of buttons) expect(button).toHaveAttribute('aria-pressed', 'false')
    await user.hover(buttons[0]!)
    expect(await screen.findByText('Mixed')).toBeInTheDocument()
    expect(screen.getByText('Mixed').parentElement).toHaveTextContent(/^Align leftMixed$/)

    await user.click(buttons[1]!)

    expect(edits).toEqual([
      restyled([
        ['one', { align: 'center' }],
        ['two', { align: 'center' }],
      ]),
    ])
    // Every text takes it, so it shows pressed while the edit is on its way.
    expect(buttons[1]).toHaveAttribute('aria-pressed', 'true')
    expect(buttons[0]).toHaveAttribute('aria-pressed', 'false')
  })

  it('keeps an alignment that differs unpressed while another edit is on its way', async () => {
    const { edits, bar, user } = await renderBar(
      [text('one', { style: { align: 'start' } }), text('two', { style: { align: 'end' } })],
      { pending: true },
    )

    await user.click(bar.getByRole('button', { name: 'Italic' }))

    expect(edits).toEqual([
      restyled([
        ['one', { chars: { italic: true } }],
        ['two', { chars: { italic: true } }],
      ]),
    ])
    expect(bar.getByRole('button', { name: 'Italic' })).toHaveAttribute('aria-pressed', 'true')
    for (const name of ['Align left', 'Align center', 'Align right']) {
      expect(bar.getByRole('button', { name })).toHaveAttribute('aria-pressed', 'false')
    }
  })

  it('presses the alignment they share, and adds nothing to its tooltip', async () => {
    const { bar, user } = await renderBar([
      text('one', { style: { align: 'center' } }),
      // A horizontal text with none is centred too.
      text('two'),
    ])

    const center = bar.getByRole('button', { name: 'Align center' })
    expect(center).toHaveAttribute('aria-pressed', 'true')
    await user.hover(center)
    const tip = await screen.findByText('Align center', { selector: '[data-slot=tooltip-content]' })
    expect(tip).toHaveTextContent(/^Align center$/)
  })

  it('shows vertical unpressed while the directions differ, and a press makes every one vertical', async () => {
    const { edits, bar, user } = await renderBar([
      text('one', { direction: 'vertical' }),
      text('two', { direction: 'horizontal' }),
    ])

    const vertical = bar.getByRole('button', { name: 'Vertical' })
    expect(vertical).toHaveAttribute('aria-pressed', 'false')
    await user.hover(vertical)
    expect(
      await screen.findByText('Text direction: mixed; click to make every selected box vertical'),
    ).toBeInTheDocument()

    await user.click(vertical)

    // Koharu's update writes each text in a step of its own.
    expect(edits.flatMap((edit) => edit.ops)).toEqual([
      { op: 'update_layer', layer: 'one', patch: { direction: 'vertical' } },
      { op: 'update_layer', layer: 'two', patch: { direction: 'vertical' } },
    ])
  })

  it('shows the size field empty, Mixed, with Auto unchosen, while the fits differ', async () => {
    const { bar, user } = await renderBar([
      text('one', { frame: box('best') }),
      text('two', { frame: box('shrink') }),
    ])

    const size = bar.getByTestId('type-size')
    expect(size).toHaveValue('')
    expect(size).toHaveAttribute('placeholder', 'Mixed')
    await user.click(bar.getByRole('button', { name: 'Choose font size' }))
    expect(await screen.findByRole('menuitemradio', { name: 'Auto' })).toHaveAttribute(
      'aria-checked',
      'false',
    )
    expect(screen.getByRole('menuitemradio', { name: '24' })).toHaveAttribute(
      'aria-checked',
      'false',
    )
  })

  it('says Mixed in the size field while the sizes differ', async () => {
    await renderBar([styled('one', { size: 20 }), styled('two', { size: 30 })])
    expect(screen.getByTestId('type-size')).toHaveAttribute('placeholder', 'Mixed')
  })

  it('says Auto, not Mixed, while every box fits its text by the best size', async () => {
    await renderBar([text('one', { frame: box('best') }), text('two', { frame: box('best') })])
    const size = screen.getByTestId('type-size')
    expect(size).toHaveValue('')
    expect(size).toHaveAttribute('placeholder', 'Auto')
  })

  it('says Mixed in the outline method while the methods differ', async () => {
    const { user } = await renderBar([
      styled('one', { stroke: WHITE, stroke_width: 2 }),
      text('two', {
        style: { stroke_method: 'contour', chars: { stroke: WHITE, stroke_width: 2 } },
      }),
    ])

    await openExtras(user)
    expect(await screen.findByRole('combobox', { name: 'Outline method' })).toHaveTextContent(
      'Mixed',
    )
  })

  it('shows what the lettered texts share with a block that has no translation first', async () => {
    const { edits, bar, user } = await renderBar(
      [
        block('first', 'やあ'),
        text('two', {
          direction: 'vertical',
          frame: box('best'),
          style: { align: 'end', line_height: 1.5, stroke_method: 'contour' },
        }),
      ],
      { pending: true },
    )

    // The paragraph of the text, as with the block after it, never a new text's.
    expect(bar.getByRole('button', { name: 'Align bottom' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    expect(bar.getByRole('button', { name: 'Vertical' })).toHaveAttribute('aria-pressed', 'true')
    expect(bar.getByTestId('type-size')).toHaveAttribute('placeholder', 'Auto')
    await openExtras(user)
    expect(await screen.findByRole('combobox', { name: 'Outline method' })).toHaveTextContent(
      'Contour stroke',
    )
    await user.keyboard('{Escape}')

    await user.click(bar.getByRole('button', { name: 'Spacing' }))
    const spacing = await screen.findByRole('textbox', { name: 'Line spacing' })
    expect(spacing).toHaveValue('1.5')
    // A step goes on from the spacing shown.
    fireEvent.keyDown(spacing, { key: 'ArrowUp' })
    await waitFor(() =>
      expect(edits.flatMap((edit) => edit.ops)).toContainEqual({
        op: 'update_layer',
        layer: 'two',
        patch: { style: { line_height: 1.55 } },
      }),
    )
  })
})

describe('Booharu type bar borders over texts whose borders differ (S11-h)', () => {
  it('keeps each box’s width when a colour is picked, the default only where one has none, in one step', async () => {
    const { edits, user } = await renderBar([
      styled('one', { stroke: WHITE, stroke_width: 2 }),
      styled('two', { stroke: WHITE, stroke_width: 4 }),
      styled('three', {}),
    ])

    // Koharu's border width, beside its colour, empty while the widths differ.
    const width = screen.getByRole('textbox', { name: 'Border width' })
    expect(width).toHaveValue('')
    expect(width).toHaveAttribute('placeholder', 'Mixed')
    await openWell(user)
    pickColor('#FF0000')

    await waitFor(() =>
      expect(edits).toEqual([
        restyled([
          ['one', { chars: { stroke: RED } }],
          ['two', { chars: { stroke: RED } }],
          ['three', { chars: { stroke: RED, stroke_width: 1.5 } }],
        ]),
      ]),
    )
  })

  it('keeps each box’s colour when a width is set, white only where one has none, in one step', async () => {
    const { edits, user } = await renderBar([
      styled('one', { stroke: WHITE, stroke_width: 2 }),
      styled('two', { stroke: BLACK, stroke_width: 4 }),
      styled('three', {}),
    ])

    // The colours differ: the well shows none.
    expect(screen.getByRole('button', { name: 'Border color' }).firstElementChild).not.toHaveStyle({
      backgroundColor: '#FFFFFF',
    })
    await typeWidth(user, '6')

    await waitFor(() =>
      expect(edits).toEqual([
        restyled([
          ['one', { chars: { stroke_width: 6 } }],
          ['two', { chars: { stroke_width: 6 } }],
          ['three', { chars: { stroke_width: 6, stroke: WHITE } }],
        ]),
      ]),
    )
  })

  it('shows the colour the boxes share while only their widths differ', async () => {
    const { user } = await renderBar([
      styled('one', { stroke: RED, stroke_width: 2 }),
      styled('two', { stroke: RED, stroke_width: 4 }),
    ])

    await openWell(user)
    expect(screen.getByRole('textbox', { name: 'Hex color code' })).toHaveValue('#FF0000')
  })

  it('says Mixed while the widths differ in a border width field as wide as the size field (V1)', async () => {
    const { bar } = await renderBar([
      styled('one', { stroke: WHITE, stroke_width: 2 }),
      styled('two', { stroke: WHITE, stroke_width: 4 }),
    ])

    const width = bar.getByRole('textbox', { name: 'Border width' })
    expect(width).toHaveValue('')
    expect(width).toHaveAttribute('placeholder', 'Mixed')
    // Booharu's marked wrapper gives it the size field's w-15, in which "Mixed" fits in English,
    // for Koharu's w-12, which cuts it to "Mi…".
    const size = bar.getByTestId('type-size')
    expect(size.closest('.w-15')).not.toBeNull()
    const wrapper = width.closest('.w-15')
    expect(wrapper).not.toBeNull()
    expect(wrapper).not.toContainElement(size)
    expect(width.closest('.w-12')).toBeNull()
  })

  it('keeps each box’s second border colour and width likewise, black and 3 where one has none', async () => {
    const first = { stroke: WHITE, stroke_width: 2 }
    const { edits, user } = await renderBar([
      styled('one', { ...first, outer_stroke: RED, outer_stroke_width: 5 }),
      styled('two', { ...first, outer_stroke: WHITE, outer_stroke_width: 8 }),
      styled('three', first),
    ])

    await openExtras(user)
    const width = await screen.findByRole('textbox', { name: 'Second border width' })
    expect(width).toHaveValue('')
    expect(width).toHaveAttribute('placeholder', 'Mixed')
    await user.type(width, '6{Enter}')
    await waitFor(() => expect(edits).toHaveLength(1))
    await openWell(user, 'Second border color')
    pickColor('#000000')

    await waitFor(() => expect(edits).toHaveLength(2))
    expect(edits).toEqual([
      restyled([
        ['one', { chars: { outer_stroke_width: 6 } }],
        ['two', { chars: { outer_stroke_width: 6 } }],
        ['three', { chars: { outer_stroke_width: 6, outer_stroke: BLACK } }],
      ]),
      restyled([
        ['one', { chars: { outer_stroke: BLACK } }],
        ['two', { chars: { outer_stroke: BLACK } }],
        ['three', { chars: { outer_stroke: BLACK, outer_stroke_width: 3 } }],
      ]),
    ])
  })
})

describe('Booharu type bar borders for one text (S11-h)', () => {
  it('sends the width with a colour only while the characters share one', async () => {
    const { edits, user } = await renderBar([
      text('one', {
        style: { chars: { stroke: WHITE, stroke_width: 2 } },
        content: [{ insert: 'Hel', attributes: { stroke_width: 4 } }, { insert: 'lo' }],
      }),
    ])

    await openWell(user)
    pickColor('#FF0000')

    // The characters keep their widths.
    await waitFor(() => expect(edits).toEqual([restyled([['one', { chars: { stroke: RED } }]])]))
  })

  it('sends the shared width with a colour, as Koharu does', async () => {
    const { edits, user } = await renderBar([styled('one', { stroke: WHITE, stroke_width: 2 })])

    await openWell(user)
    pickColor('#FF0000')

    await waitFor(() =>
      expect(edits).toEqual([restyled([['one', { chars: { stroke: RED, stroke_width: 2 } }]])]),
    )
  })

  it('styles the characters typed with a width alone while their colours differ', async () => {
    const { user } = await renderBar([styled('one', { stroke: WHITE, stroke_width: 2 })], {
      typing: { layer: 'one', chars: { stroke_width: 2 }, mixed: ['stroke'] },
    })

    await typeWidth(user, '5')

    await waitFor(() => expect(commands.styleTyping).toHaveBeenCalled())
    await settled()
    expect(commands.styleTyping).toHaveBeenCalledExactlyOnceWith({ stroke_width: 5 })
  })

  it('styles the characters typed with their colour and a width while they share a colour', async () => {
    const { user } = await renderBar([styled('one', {})], {
      typing: { layer: 'one', chars: { stroke: BLACK, stroke_width: 2 }, mixed: [] },
    })

    await typeWidth(user, '5')

    await waitFor(() => expect(commands.styleTyping).toHaveBeenCalled())
    await settled()
    expect(commands.styleTyping).toHaveBeenCalledExactlyOnceWith({
      stroke_width: 5,
      stroke: BLACK,
    })
  })
})

// Koharu's border colour and width stay in the bar, beside More border options: for one text whose
// characters share their border, they write what Koharu's write.
describe("Koharu's border colour and width in the Booharu type bar, for one text", () => {
  it('borders a text with none in a picked colour, 1.5 wide', async () => {
    const { edits, user } = await renderBar([styled('one', {})])

    await openWell(user)
    pickColor('#FF0000')

    await waitFor(() =>
      expect(edits).toEqual([restyled([['one', { chars: { stroke: RED, stroke_width: 1.5 } }]])]),
    )
  })

  it('takes the border off with Transparent', async () => {
    const { edits, user } = await renderBar([styled('one', { stroke: RED, stroke_width: 2 })])

    await openWell(user)
    await user.click(await screen.findByRole('button', { name: 'Transparent' }))

    await waitFor(() => expect(edits).toEqual([restyled([['one', { chars: { stroke: null } }]])]))
  })

  it('writes a typed width with the border’s colour once Enter leaves the field, nothing before', async () => {
    const { edits, user } = await renderBar([styled('one', { stroke: RED, stroke_width: 2 })])

    const width = screen.getByRole('textbox', { name: 'Border width' })
    expect(width).toHaveValue('2')
    await user.clear(width)
    await user.type(width, '6')
    await settled()
    expect(edits).toEqual([])
    await user.keyboard('{Enter}')

    expect(width).not.toHaveFocus()
    await waitFor(() => expect(edits).toHaveLength(1))
    await settled()
    expect(edits).toEqual([restyled([['one', { chars: { stroke_width: 6, stroke: RED } }]])])
  })

  it('writes a typed width as the field is left, in white on a text with no border', async () => {
    const { edits, user } = await renderBar([styled('one', {})])

    const width = screen.getByRole('textbox', { name: 'Border width' })
    expect(width).toHaveValue('1.5')
    await user.clear(width)
    await user.type(width, '4')
    await user.tab()

    await waitFor(() => expect(edits).toHaveLength(1))
    await settled()
    expect(edits).toEqual([restyled([['one', { chars: { stroke_width: 4, stroke: WHITE } }]])])
  })

  it('writes a width picked from its menu with the border’s colour', async () => {
    const { edits, user } = await renderBar([styled('one', { stroke: RED, stroke_width: 2 })])

    await user.click(screen.getByRole('button', { name: 'Choose border width' }))
    await user.click(await screen.findByRole('menuitemradio', { name: '8' }))

    await waitFor(() =>
      expect(edits).toEqual([restyled([['one', { chars: { stroke_width: 8, stroke: RED } }]])]),
    )
  })
})

describe('Booharu B and I over characters that differ (S11-h)', () => {
  it('shows B and I unpressed, their tooltips saying a press turns each on for all', async () => {
    const { edits, bar, user } = await renderBar([
      styled('one', { weight: 400, italic: true }),
      styled('two', { weight: 700, italic: false }),
    ])

    const bold = bar.getByRole('button', { name: 'Bold' })
    const italic = bar.getByRole('button', { name: 'Italic' })
    expect(bold).toHaveAttribute('aria-pressed', 'false')
    expect(italic).toHaveAttribute('aria-pressed', 'false')
    await user.hover(bold)
    expect(await screen.findByText('Bold: mixed; click to turn it on for all')).toBeInTheDocument()
    await user.hover(italic)
    expect(
      await screen.findByText('Italic: mixed; click to turn it on for all'),
    ).toBeInTheDocument()

    await user.click(italic)
    expect(edits).toEqual([
      restyled([
        ['one', { chars: { italic: true } }],
        ['two', { chars: { italic: true } }],
      ]),
    ])
  })

  it('names B and I alone while the characters share them', async () => {
    const { bar, user } = await renderBar([
      styled('one', { weight: 700 }),
      styled('two', { weight: 700 }),
    ])

    await user.hover(bar.getByRole('button', { name: 'Bold' }))
    const tip = await screen.findByText('Bold', { selector: '[data-slot=tooltip-content]' })
    expect(tip).toHaveTextContent(/^Bold$/)
  })
})
