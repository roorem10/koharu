import { fireEvent, screen } from '@testing-library/react'
import type userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { commands, type CharStyle } from '@koharu/ipc/protocol'

import { renderBar, restyled, styled, text } from './typeBar'

const WHITE: CharStyle['stroke'] = [1, 1, 1, 1]
const BLACK: CharStyle['stroke'] = [0, 0, 0, 1]
const RED: CharStyle['stroke'] = [1, 0, 0, 1]

const box = (fit?: 'shrink' | 'best') => ({
  box: { bounds: { x: 0, y: 0, width: 100, height: 50 }, ...(fit ? { fit } : {}) },
})

/** Opens the border popover, and the colour picker of the well named `well` when given. */
async function openBorder(user: ReturnType<typeof userEvent.setup>, well?: string) {
  await user.click(screen.getByRole('button', { name: 'Border' }))
  if (well) await user.click(await screen.findByRole('button', { name: well }))
}

/** Types `hex` into the open colour picker, which picks it. */
function pickColor(hex: string) {
  fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
    target: { value: hex },
  })
}

describe('Booharu type bar over texts whose paragraphs differ (S11-h)', () => {
  it('presses no alignment while they differ, and a click writes the alignment alone to all', async () => {
    const { edits, bar, user } = renderBar(
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
    const { edits, bar, user } = renderBar(
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
    const { bar, user } = renderBar([
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
    const { edits, bar, user } = renderBar([
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
    const { bar, user } = renderBar([
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

  it('says Mixed in the size field while the sizes differ', () => {
    renderBar([styled('one', { size: 20 }), styled('two', { size: 30 })])
    expect(screen.getByTestId('type-size')).toHaveAttribute('placeholder', 'Mixed')
  })

  it('says Auto, not Mixed, while every box fits its text by the best size', () => {
    renderBar([text('one', { frame: box('best') }), text('two', { frame: box('best') })])
    const size = screen.getByTestId('type-size')
    expect(size).toHaveValue('')
    expect(size).toHaveAttribute('placeholder', 'Auto')
  })

  it('says Mixed in the outline method while the methods differ', async () => {
    const { user } = renderBar([
      styled('one', { stroke: WHITE, stroke_width: 2 }),
      text('two', {
        style: { stroke_method: 'expansion', chars: { stroke: WHITE, stroke_width: 2 } },
      }),
    ])

    await openBorder(user)
    expect(await screen.findByRole('combobox', { name: 'Outline method' })).toHaveTextContent(
      'Mixed',
    )
  })
})

describe('Booharu type bar borders over texts whose borders differ (S11-h)', () => {
  it('keeps each box’s width when a colour is picked, the default only where one has none, in one step', async () => {
    const { edits, user } = renderBar([
      styled('one', { stroke: WHITE, stroke_width: 2 }),
      styled('two', { stroke: WHITE, stroke_width: 4 }),
      styled('three', {}),
    ])

    await openBorder(user)
    const width = await screen.findByRole('textbox', { name: 'Border width' })
    expect(width).toHaveValue('')
    expect(width).toHaveAttribute('placeholder', 'Mixed')
    await user.click(screen.getByRole('button', { name: 'Border color' }))
    pickColor('#FF0000')

    expect(edits).toEqual([
      restyled([
        ['one', { chars: { stroke: RED } }],
        ['two', { chars: { stroke: RED } }],
        ['three', { chars: { stroke: RED, stroke_width: 1.5 } }],
      ]),
    ])
  })

  it('keeps each box’s colour when a width is set, white only where one has none, in one step', async () => {
    const { edits, user } = renderBar([
      styled('one', { stroke: WHITE, stroke_width: 2 }),
      styled('two', { stroke: BLACK, stroke_width: 4 }),
      styled('three', {}),
    ])

    await openBorder(user)
    // The colours differ: the well shows none.
    expect(screen.getByRole('button', { name: 'Border color' }).firstElementChild).not.toHaveStyle({
      backgroundColor: '#FFFFFF',
    })
    fireEvent.change(await screen.findByRole('textbox', { name: 'Border width' }), {
      target: { value: '6' },
    })

    expect(edits).toEqual([
      restyled([
        ['one', { chars: { stroke_width: 6 } }],
        ['two', { chars: { stroke_width: 6 } }],
        ['three', { chars: { stroke_width: 6, stroke: WHITE } }],
      ]),
    ])
  })

  it('shows the colour the boxes share while only their widths differ', async () => {
    const { user } = renderBar([
      styled('one', { stroke: RED, stroke_width: 2 }),
      styled('two', { stroke: RED, stroke_width: 4 }),
    ])

    await openBorder(user, 'Border color')
    expect(screen.getByRole('textbox', { name: 'Hex color code' })).toHaveValue('#FF0000')
  })

  it('keeps each box’s second border colour and width likewise, black and 3 where one has none', async () => {
    const first = { stroke: WHITE, stroke_width: 2 }
    const { edits, user } = renderBar([
      styled('one', { ...first, outer_stroke: RED, outer_stroke_width: 5 }),
      styled('two', { ...first, outer_stroke: WHITE, outer_stroke_width: 8 }),
      styled('three', first),
    ])

    await openBorder(user)
    const width = await screen.findByRole('textbox', { name: 'Second border width' })
    expect(width).toHaveValue('')
    expect(width).toHaveAttribute('placeholder', 'Mixed')
    fireEvent.change(width, { target: { value: '6' } })
    await user.click(screen.getByRole('button', { name: 'Second border color' }))
    pickColor('#000000')

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
    const { edits, user } = renderBar([
      text('one', {
        style: { chars: { stroke: WHITE, stroke_width: 2 } },
        content: [{ insert: 'Hel', attributes: { stroke_width: 4 } }, { insert: 'lo' }],
      }),
    ])

    await openBorder(user, 'Border color')
    pickColor('#FF0000')

    // The characters keep their widths.
    expect(edits).toEqual([restyled([['one', { chars: { stroke: RED } }]])])
  })

  it('sends the shared width with a colour, as Koharu does', async () => {
    const { edits, user } = renderBar([styled('one', { stroke: WHITE, stroke_width: 2 })])

    await openBorder(user, 'Border color')
    pickColor('#FF0000')

    expect(edits).toEqual([restyled([['one', { chars: { stroke: RED, stroke_width: 2 } }]])])
  })

  it('styles the characters typed with a width alone while their colours differ', async () => {
    const { user } = renderBar([styled('one', { stroke: WHITE, stroke_width: 2 })], {
      typing: { layer: 'one', chars: { stroke_width: 2 }, mixed: ['stroke'] },
    })

    await openBorder(user)
    fireEvent.change(await screen.findByRole('textbox', { name: 'Border width' }), {
      target: { value: '5' },
    })

    expect(commands.styleTyping).toHaveBeenCalledExactlyOnceWith({ stroke_width: 5 })
  })

  it('styles the characters typed with their colour and a width while they share a colour', async () => {
    const { user } = renderBar([styled('one', {})], {
      typing: { layer: 'one', chars: { stroke: BLACK, stroke_width: 2 }, mixed: [] },
    })

    await openBorder(user)
    fireEvent.change(await screen.findByRole('textbox', { name: 'Border width' }), {
      target: { value: '5' },
    })

    expect(commands.styleTyping).toHaveBeenCalledExactlyOnceWith({
      stroke_width: 5,
      stroke: BLACK,
    })
  })
})

describe('Booharu B and I over characters that differ (S11-h)', () => {
  it('shows B and I unpressed, their tooltips saying a press turns each on for all', async () => {
    const { edits, bar, user } = renderBar([
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
    const { bar, user } = renderBar([
      styled('one', { weight: 700 }),
      styled('two', { weight: 700 }),
    ])

    await user.hover(bar.getByRole('button', { name: 'Bold' }))
    const tip = await screen.findByText('Bold', { selector: '[data-slot=tooltip-content]' })
    expect(tip).toHaveTextContent(/^Bold$/)
  })
})
