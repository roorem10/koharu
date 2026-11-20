import { act, fireEvent, screen, waitFor, within } from '@testing-library/react'
import type userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { openFx, useFx } from '@/lib/booharu/fx'
import { seedGradient } from '@/lib/booharu/paint'
import { setWordRange, type WordRange } from '@/lib/booharu/words'
import { pageKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type CharStyle,
  type Gradient,
  type Page,
  type Rgba,
  type Typed,
} from '@koharu/ipc/protocol'

import { block } from './boxes'
import { renderBar, restyled, styled, text } from './typeBar'

const RED: Rgba = [1, 0, 0, 1]
const GREEN: Rgba = [0, 1, 0, 1]
const BLUE: Rgba = [0, 0, 1, 1]

/** A linear gradient at 90°, red at its start, see-through by half, to blue. */
const GRADIENT: Gradient = {
  kind: 'linear',
  angle: 90,
  stops: [
    { offset: 0, color: [1, 0, 0, 0.5] },
    { offset: 1, color: BLUE },
  ],
}

/** Another gradient, from blue to red at 0°. */
const OTHER: Gradient = {
  kind: 'linear',
  angle: 0,
  stops: [
    { offset: 0, color: BLUE },
    { offset: 1, color: RED },
  ],
}

/** "world", chosen in the Story panel in the text `text` reading "Hello world". */
const world: WordRange = { layer: 'text', start: 6, end: 11, text: 'Hello world' }

type User = ReturnType<typeof userEvent.setup>

/** A text layer `text` reading "Hello world", styled `chars`, "world" marked `marks` when given. */
function hello(chars: CharStyle, marks?: CharStyle) {
  return text('text', {
    style: { chars },
    content: marks
      ? [{ insert: 'Hello ' }, { insert: 'world', attributes: marks }]
      : [{ insert: 'Hello world' }],
  })
}

/** The canvas typing in `text`, its characters sharing `chars`. */
function typing(chars: CharStyle): Typed {
  return { layer: 'text', chars, mixed: [] } as Typed
}

/** Opens Text effects from the type bar's fx button and gives its popover. */
async function openEffects(user: User) {
  await user.click(screen.getByRole('button', { name: 'Text effects' }))
  return within(await screen.findByRole('dialog', { name: 'Text effects' }))
}

/** Types `hex` into the open colour picker, which picks it. */
function pickColor(hex: string) {
  fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
    target: { value: hex },
  })
}

const tooltip = (label: string) =>
  screen.findByText(label, { selector: '[data-slot=tooltip-content]' })

describe('Booharu Text effects in the type bar (S12-k)', () => {
  it('is an italic fx after the border, Done staying last', async () => {
    const { bar, user } = renderBar([hello({})], { typing: typing({}) })

    const buttons = bar.getAllByRole('button')
    const fx = bar.getByRole('button', { name: 'Text effects' })
    expect(buttons.indexOf(fx)).toBe(
      buttons.indexOf(bar.getByRole('button', { name: 'Border' })) + 1,
    )
    expect(buttons.at(-1)).toBe(bar.getByRole('button', { name: 'Done' }))
    expect(fx).toHaveTextContent('fx')
    expect(fx.querySelector('.italic')).not.toBeNull()
    await user.hover(fx)
    expect(await tooltip('Text effects')).toBeInTheDocument()
  })

  it.each([
    [
      'every character shows a gradient',
      [styled('box', { fill: RED, fill_gradient: GRADIENT })],
      'true',
    ],
    ['some characters show one', [hello({ fill: RED }, { fill_gradient: GRADIENT })], 'true'],
    [
      'one box of two shows one',
      [styled('one', { fill_gradient: GRADIENT }), styled('two', {})],
      'true',
    ],
    ['no character shows one', [styled('box', { fill: RED })], 'false'],
  ])('is pressed while %s', (_, layers, pressed) => {
    const { bar } = renderBar(layers)

    expect(bar.getByRole('button', { name: 'Text effects' })).toHaveAttribute(
      'aria-pressed',
      pressed,
    )
  })

  it('opens a popover titled Text effects, the effects still to come shown off', async () => {
    const { user } = renderBar([styled('box', { fill: RED })])

    const effects = await openEffects(user)

    expect(effects.getByRole('heading', { name: 'Text effects' })).toBeInTheDocument()
    for (const later of ['Screentone fill (later)', 'Wavy / shaky text (later)']) {
      expect(effects.getByRole('switch', { name: later })).toHaveAttribute('aria-disabled', 'true')
    }
    expect(useFx.getState().open).toBe(true)
  })

  it('opens from openFx, as a box’s menu opens it, and closes when nothing is selected', async () => {
    renderBar([styled('box', { fill: RED })])

    act(() => openFx())
    expect(await screen.findByRole('dialog', { name: 'Text effects' })).toBeInTheDocument()

    act(() => useKoharuStore.setState({ selectedLayers: [] }))
    expect(useFx.getState().open).toBe(false)
  })

  it('closes when the bar shows another page', async () => {
    renderBar([styled('box', { fill: RED })])
    act(() => openFx())
    expect(await screen.findByRole('dialog', { name: 'Text effects' })).toBeInTheDocument()
    expect(useFx.getState().page).toBe('page')

    const page = queryClient.getQueryData<Page>(pageKey)
    act(() => queryClient.setQueryData(pageKey, { ...page, id: 'next' }))

    await waitFor(() => expect(useFx.getState().open).toBe(false))
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Text effects' })).toBeNull())
  })
})

describe('Booharu Gradient fill in Text effects (S12-c, S12-k)', () => {
  it('is on while every character shows a gradient, with its kind, angle and ends', async () => {
    const { user } = renderBar([
      styled('one', { fill_gradient: GRADIENT }),
      styled('two', { fill: GREEN, fill_gradient: GRADIENT }),
    ])

    const effects = await openEffects(user)

    expect(effects.getByRole('switch', { name: 'Gradient fill' })).toBeChecked()
    expect(effects.queryByText('Mixed')).toBeNull()
    expect(effects.getByRole('combobox', { name: 'Gradient kind' })).toHaveTextContent('Linear')
    expect(effects.getByRole('textbox', { name: 'Gradient angle' })).toHaveValue('90')
    expect(effects.getByRole('button', { name: 'Start color' })).toBeInTheDocument()
    expect(effects.getByRole('button', { name: 'End color' })).toBeInTheDocument()
  })

  it('is off, with no gradient controls, while no character shows one', async () => {
    const { user } = renderBar([styled('box', { fill: RED })])

    const effects = await openEffects(user)

    expect(effects.getByRole('switch', { name: 'Gradient fill' })).not.toBeChecked()
    expect(effects.queryByRole('combobox', { name: 'Gradient kind' })).toBeNull()
  })

  it('says Mixed while only some characters show one, and leaves the controls out', async () => {
    const { user } = renderBar([hello({ fill: RED, fill_gradient: GRADIENT }, { fill: GREEN })])

    const effects = await openEffects(user)

    const toggle = effects.getByRole('switch', { name: 'Gradient fill' })
    expect(toggle).not.toBeChecked()
    expect(toggle).toHaveAccessibleDescription('Mixed')
    expect(effects.queryByRole('combobox', { name: 'Gradient kind' })).toBeNull()
  })

  it('is on, without controls, while every box shows a gradient of its own', async () => {
    const { user } = renderBar([
      styled('one', { fill_gradient: GRADIENT }),
      styled('two', { fill_gradient: OTHER }),
    ])

    const effects = await openEffects(user)

    expect(effects.getByRole('switch', { name: 'Gradient fill' })).toBeChecked()
    expect(effects.queryByText('Mixed')).toBeNull()
    expect(effects.queryByRole('combobox', { name: 'Gradient kind' })).toBeNull()
  })

  it('turns on with a gradient seeded from the box’s colour, shown at once', async () => {
    const { edits, user } = renderBar([styled('box', { fill: RED })], { pending: true })

    const effects = await openEffects(user)
    await user.click(effects.getByRole('switch', { name: 'Gradient fill' }))

    expect(edits).toEqual([restyled([['box', { chars: { fill_gradient: seedGradient(RED) } }]])])
    expect(effects.getByRole('switch', { name: 'Gradient fill' })).toBeChecked()
    expect(effects.getByRole('textbox', { name: 'Gradient angle' })).toHaveValue('90')
    expect(screen.getByRole('button', { name: 'Text effects' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
  })

  it('gives several boxes each a gradient from its own colour in one step, one with a gradient keeping its own', async () => {
    const { edits, user } = renderBar([
      styled('one', { fill: RED }),
      styled('two', { fill: BLUE }),
      styled('three', { fill_gradient: OTHER }),
      block('first', 'やあ'),
    ])

    const effects = await openEffects(user)
    const toggle = effects.getByRole('switch', { name: 'Gradient fill' })
    expect(toggle).toHaveAccessibleDescription('Mixed')
    await user.click(toggle)

    expect(edits).toEqual([
      {
        label: 'set_style',
        ops: [
          {
            op: 'update_layer',
            layer: 'one',
            patch: { style: { chars: { fill_gradient: seedGradient(RED) } } },
          },
          {
            op: 'update_layer',
            layer: 'two',
            patch: { style: { chars: { fill_gradient: seedGradient(BLUE) } } },
          },
          // A block with no translation yet letters black.
          {
            op: 'update_layer',
            layer: 'first',
            patch: {
              translation: { style: { chars: { fill_gradient: seedGradient([0, 0, 0, 1]) } } },
            },
          },
        ],
      },
    ])
  })

  it('gives a box back the gradient its style has, which a word’s colour hid', async () => {
    const { edits, user } = renderBar([hello({ fill_gradient: GRADIENT }, { fill: GREEN })])

    const effects = await openEffects(user)
    await user.click(effects.getByRole('switch', { name: 'Gradient fill' }))

    expect(edits).toEqual([
      {
        label: 'set_style',
        ops: [
          {
            op: 'update_layer',
            layer: 'text',
            patch: { style: { chars: { fill_gradient: GRADIENT } } },
          },
          {
            op: 'set_rich_text',
            layer: 'text',
            spans: [
              { insert: 'Hello ', attributes: {} },
              { insert: 'world', attributes: {} },
            ],
          },
        ],
      },
    ])
  })

  it('turns on for words chosen in Story, from the colour they show', async () => {
    const { edits, user } = renderBar([hello({ fill: BLUE }, { fill: RED })])
    act(() => setWordRange(world))

    const effects = await openEffects(user)
    await user.click(effects.getByRole('switch', { name: 'Gradient fill' }))

    expect(commands.booharuStyleWords).toHaveBeenCalledExactlyOnceWith(
      'text',
      6,
      11,
      'Hello world',
      { set: { fill_gradient: seedGradient(RED) } },
    )
    expect(edits).toEqual([])
  })

  it('turns on for the characters typed, from the colour they share', async () => {
    const { edits, user } = renderBar([hello({})], { typing: typing({ fill: BLUE }) })

    const effects = await openEffects(user)
    await user.click(effects.getByRole('switch', { name: 'Gradient fill' }))

    expect(commands.styleTyping).toHaveBeenCalledExactlyOnceWith({
      fill_gradient: seedGradient(BLUE),
    })
    expect(edits).toEqual([])
  })

  it('turns off by taking the gradient off whole boxes, their colours showing again', async () => {
    const { edits, user } = renderBar(
      [
        styled('one', { fill: RED, fill_gradient: GRADIENT }),
        styled('two', { fill: BLUE, fill_gradient: OTHER }),
      ],
      { pending: true },
    )

    const effects = await openEffects(user)
    await user.click(effects.getByRole('switch', { name: 'Gradient fill' }))

    expect(edits).toEqual([
      restyled([
        ['one', { chars: { fill_gradient: null } }],
        ['two', { chars: { fill_gradient: null } }],
      ]),
    ])
    expect(effects.getByRole('switch', { name: 'Gradient fill' })).not.toBeChecked()
  })

  it('turns off for words chosen in Story by giving them the colour they show', async () => {
    const { user } = renderBar([hello({ fill: RED, fill_gradient: GRADIENT })])
    act(() => setWordRange(world))

    const effects = await openEffects(user)
    await user.click(effects.getByRole('switch', { name: 'Gradient fill' }))

    expect(commands.booharuStyleWords).toHaveBeenCalledExactlyOnceWith(
      'text',
      6,
      11,
      'Hello world',
      { set: { fill: RED } },
    )
  })

  it('turns off for the characters typed by giving them their colour', async () => {
    const { user } = renderBar([hello({})], {
      typing: typing({ fill: BLUE, fill_gradient: GRADIENT }),
    })

    const effects = await openEffects(user)
    await user.click(effects.getByRole('switch', { name: 'Gradient fill' }))

    expect(commands.styleTyping).toHaveBeenCalledExactlyOnceWith({ fill: BLUE })
  })
})

describe('Booharu shared gradient controls in Text effects (S12-k)', () => {
  /** Boxes sharing GRADIENT, and a block with no translation, so no gradient, among them. */
  const sharing = () => [
    styled('one', { fill_gradient: GRADIENT }),
    styled('two', { fill: RED, fill_gradient: GRADIENT }),
    block('first', 'やあ'),
  ]

  /** One edit giving `gradient` to the boxes sharing GRADIENT, the block left be. */
  const shared = (gradient: Gradient) =>
    restyled([
      ['one', { chars: { fill_gradient: gradient } }],
      ['two', { chars: { fill_gradient: gradient } }],
    ])

  it('changes the kind, the angle going off for a radial gradient', async () => {
    const { edits, user } = renderBar(sharing(), { pending: true })

    const effects = await openEffects(user)
    await user.click(effects.getByRole('combobox', { name: 'Gradient kind' }))
    await user.click(await screen.findByRole('option', { name: 'Radial' }))

    expect(edits).toEqual([shared({ ...GRADIENT, kind: 'radial' })])
    expect(effects.getByRole('textbox', { name: 'Gradient angle' })).toBeDisabled()
  })

  it('sets a typed angle on Enter, one past 180 as 180, then nothing on leaving', async () => {
    const { edits, user } = renderBar(sharing(), { pending: true })

    const effects = await openEffects(user)
    const angle = effects.getByRole('textbox', { name: 'Gradient angle' })
    await user.clear(angle)
    await user.type(angle, '45{Enter}')

    expect(edits).toEqual([shared({ ...GRADIENT, angle: 45 })])
    expect(angle).toHaveValue('45')

    await user.clear(angle)
    await user.type(angle, '400{Enter}')
    fireEvent.blur(angle)

    expect(edits).toEqual([shared({ ...GRADIENT, angle: 45 }), shared({ ...GRADIENT, angle: 180 })])
    expect(angle).toHaveValue('180')
  })

  it('colours the start and the end, each keeping its alpha', async () => {
    const { edits, user } = renderBar(sharing())

    const effects = await openEffects(user)
    await user.click(effects.getByRole('button', { name: 'Start color' }))
    pickColor('#00FF00')
    await user.keyboard('{Escape}')
    await user.click(effects.getByRole('button', { name: 'End color' }))
    pickColor('#00FF00')

    expect(edits).toEqual([
      shared({
        ...GRADIENT,
        stops: [
          { offset: 0, color: [0, 1, 0, 0.5] },
          { offset: 1, color: BLUE },
        ],
      }),
      shared({
        ...GRADIENT,
        stops: [
          { offset: 0, color: [1, 0, 0, 0.5] },
          { offset: 1, color: GREEN },
        ],
      }),
    ])
  })

  it('changes the gradient of words chosen in Story through the words edit', async () => {
    const { edits, user } = renderBar([hello({ fill_gradient: GRADIENT })])
    act(() => setWordRange(world))

    const effects = await openEffects(user)
    await user.click(effects.getByRole('combobox', { name: 'Gradient kind' }))
    await user.click(await screen.findByRole('option', { name: 'Radial' }))

    expect(commands.booharuStyleWords).toHaveBeenCalledExactlyOnceWith(
      'text',
      6,
      11,
      'Hello world',
      { set: { fill_gradient: { ...GRADIENT, kind: 'radial' } } },
    )
    expect(edits).toEqual([])
  })
})

describe('Booharu Reset to lettering default in Text effects (S12-k)', () => {
  it('resets the boxes the bar styles, as a box’s menu does', async () => {
    const { user } = renderBar([styled('one', { fill: RED }), styled('two', {})])
    vi.mocked(commands.booharuResetLettering).mockResolvedValue({
      changed: 2,
      locked: 0,
      untranslated: 0,
    })

    const effects = await openEffects(user)
    await user.click(effects.getByRole('button', { name: 'Reset to lettering default' }))

    expect(commands.booharuResetLettering).toHaveBeenCalledExactlyOnceWith(['one', 'two'])
  })

  it('is off while the canvas types, and where nothing can be reset', async () => {
    const { user } = renderBar([hello({})], { typing: typing({}) })

    const effects = await openEffects(user)
    expect(effects.getByRole('button', { name: 'Reset to lettering default' })).toBeDisabled()
  })

  it('is off for a block with no translation yet', async () => {
    const { user } = renderBar([block('first', 'やあ')])

    const effects = await openEffects(user)
    expect(effects.getByRole('button', { name: 'Reset to lettering default' })).toBeDisabled()
  })
})
