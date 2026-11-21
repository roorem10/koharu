import { act, fireEvent, screen, waitFor } from '@testing-library/react'
import type userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { setWordRange, type WordRange } from '@/lib/booharu/words'
import { commands, type CharStyle, type Typed } from '@koharu/ipc/protocol'

import { renderBar, restyled, settled, styled, text } from './typeBar'

type User = ReturnType<typeof userEvent.setup>

/** Opens the type bar's spacing and gives the number box named `field`. */
async function openSpacing(user: User, field: 'Line spacing' | 'Letter spacing') {
  await user.click(screen.getByRole('button', { name: 'Spacing' }))
  return screen.findByRole('textbox', { name: field })
}

/** Opens the type bar's spacing and gives its line spacing box. */
const openLineSpacing = (user: User) => openSpacing(user, 'Line spacing')

/** Opens the type bar's spacing and gives its letter spacing box. */
const openLetterSpacing = (user: User) => openSpacing(user, 'Letter spacing')

/** A text layer `text` reading "Hello world", styled `chars`, "world" marked `marks` when given. */
function hello(chars: CharStyle, marks?: CharStyle) {
  return text('text', {
    style: { chars },
    content: marks
      ? [{ insert: 'Hello ' }, { insert: 'world', attributes: marks }]
      : [{ insert: 'Hello world' }],
  })
}

/** "world", chosen in the Story panel in the text `text` reading "Hello world". */
const world: WordRange = { layer: 'text', start: 6, end: 11, text: 'Hello world' }

/** The canvas typing in `text`, its characters sharing `chars`. */
function typing(chars: CharStyle, layer: string | null = 'text'): Typed {
  return { layer, chars, mixed: [] } as Typed
}

describe('Booharu line spacing in the type bar (S11-i, S12-k)', () => {
  it('shows the spacing the texts share', async () => {
    const { user } = await renderBar([
      text('one', { style: { line_height: 1.5 } }),
      text('two', { style: { line_height: 1.5 } }),
    ])

    const field = await openLineSpacing(user)
    expect(field).toHaveValue('1.5')
    expect(field).not.toHaveAttribute('placeholder')
    expect(
      screen.getByText(
        'Click into a number box and turn the mouse wheel to change it. Hold Shift for bigger steps.',
      ),
    ).toBeInTheDocument()
  })

  it('shows Koharu’s 1.2 where no text sets one, and where one sets 1.2', async () => {
    const { user } = await renderBar([text('one'), text('two', { style: { line_height: 1.2 } })])

    expect(await openLineSpacing(user)).toHaveValue('1.2')
  })

  it('shows nothing, Mixed, where the spacings differ', async () => {
    const { user } = await renderBar([text('one', { style: { line_height: 1.5 } }), text('two')])

    const field = await openLineSpacing(user)
    expect(field).toHaveValue('')
    expect(field).toHaveAttribute('placeholder', 'Mixed')
  })

  it('takes the spacing off every text with Reset, in one step', async () => {
    const { edits, user } = await renderBar([
      text('one', { style: { line_height: 1.5 } }),
      text('two'),
    ])

    await openLineSpacing(user)
    await user.click(screen.getByRole('button', { name: 'Reset line spacing' }))

    expect(edits).toEqual([
      restyled([
        ['one', { line_height: null }],
        ['two', { line_height: null }],
      ]),
    ])
  })

  it('sends a typed spacing on Enter as one edit of the spacing alone, then nothing on leaving', async () => {
    const { edits, user } = await renderBar(
      [text('one', { style: { line_height: 1.5 } }), text('two')],
      {
        pending: true,
      },
    )

    const field = await openLineSpacing(user)
    await user.type(field, '1.8{Enter}')
    expect(edits).toEqual([
      restyled([
        ['one', { line_height: 1.8 }],
        ['two', { line_height: 1.8 }],
      ]),
    ])
    // The spacing set shows at once, as every text takes it.
    expect(field).toHaveValue('1.8')
    expect(field).not.toHaveAttribute('placeholder')

    fireEvent.blur(field)
    await settled()
    expect(edits).toHaveLength(1)
  })

  it('steps by 0.05 with the arrow keys, each step one edit', async () => {
    const { edits, user } = await renderBar([text('one')], { pending: true })

    const field = await openLineSpacing(user)
    fireEvent.keyDown(field, { key: 'ArrowUp' })

    // The edit goes as a mutation, a turn after the key.
    await waitFor(() => expect(edits).toEqual([restyled([['one', { line_height: 1.25 }]])]))
    expect(field).toHaveValue('1.25')
  })

  it('sends nothing for a cleared or unchanged box, which shows the spacing again', async () => {
    const { edits, user } = await renderBar([text('one', { style: { line_height: 1.5 } })])

    const field = await openLineSpacing(user)
    await user.clear(field)
    fireEvent.blur(field)
    expect(field).toHaveValue('1.5')

    await user.type(field, '{Enter}')
    await settled()
    expect(edits).toEqual([])
  })

  it('leaves the Enter ending an IME’s composing be, as Koharu’s number boxes do', async () => {
    const { edits, user } = await renderBar([text('one', { style: { line_height: 1.5 } })])

    const field = await openLineSpacing(user)
    await user.clear(field)
    await user.type(field, '1.8')
    fireEvent.keyDown(field, { key: 'Enter', isComposing: true })
    await settled()
    expect(edits).toEqual([])
    expect(field).toHaveFocus()

    // Once it's done, Enter sends the spacing and leaves the box.
    fireEvent.keyDown(field, { key: 'Enter' })
    await waitFor(() => expect(edits).toEqual([restyled([['one', { line_height: 1.8 }]])]))
    expect(field).not.toHaveFocus()
  })

  it('is off for a new text being typed, which has no paragraph yet, letter spacing staying on', async () => {
    const { user } = await renderBar([], { typing: typing({}, null), selected: [] })

    expect(await openLineSpacing(user)).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Reset line spacing' })).toBeDisabled()
    expect(screen.getByRole('textbox', { name: 'Letter spacing' })).toBeEnabled()
    expect(screen.getByRole('button', { name: 'Reset letter spacing' })).toBeEnabled()
  })

  it("is under Spacing, saying it's the whole box's while typing", async () => {
    const { bar, user } = await renderBar([hello({})], { typing: typing({}) })

    const button = bar.getByRole('button', { name: 'Spacing' })
    await user.hover(button)
    expect(
      await screen.findByText('Spacing', { selector: '[data-slot=tooltip-content]' }),
    ).toHaveTextContent(/^Spacing$/)
    await openLineSpacing(user)

    expect(screen.getByText('Line spacing')).toHaveTextContent(
      /^Line spacingApplies to the whole box$/,
    )
    expect(screen.getByText('Letter spacing')).toHaveTextContent(/^Letter spacing$/)
  })
})

describe('Booharu letter spacing in the type bar (S12-k)', () => {
  it('shows 0 where no text sets one', async () => {
    const { user } = await renderBar([styled('one', {}), styled('two', {})])

    const field = await openLetterSpacing(user)
    expect(field).toHaveValue('0')
    expect(field).not.toHaveAttribute('placeholder')
  })

  it('shows the spacing the texts share', async () => {
    const { user } = await renderBar([
      styled('one', { tracking: 50 }),
      styled('two', { tracking: 50 }),
    ])

    expect(await openLetterSpacing(user)).toHaveValue('50')
  })

  it('shows nothing, Mixed, where the spacings differ, a word’s own among them', async () => {
    const { user } = await renderBar([hello({ tracking: 50 }, { tracking: -20 })])

    const field = await openLetterSpacing(user)
    expect(field).toHaveValue('')
    expect(field).toHaveAttribute('placeholder', 'Mixed')
  })

  it('sets a typed spacing on every text in one step, shown at once', async () => {
    const { edits, user } = await renderBar(
      [styled('one', { tracking: 50 }), hello({}, { tracking: -20 })],
      { pending: true },
    )

    const field = await openLetterSpacing(user)
    await user.type(field, '120{Enter}')

    // The word's own spacing comes off with it, as every style edit takes a key's marks off.
    expect(edits).toEqual([
      {
        label: 'set_style',
        ops: [
          { op: 'update_layer', layer: 'one', patch: { style: { chars: { tracking: 120 } } } },
          { op: 'update_layer', layer: 'text', patch: { style: { chars: { tracking: 120 } } } },
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
    expect(field).toHaveValue('120')
    expect(field).not.toHaveAttribute('placeholder')
  })

  it('steps by 10 with the arrow keys, each step one edit', async () => {
    const { edits, user } = await renderBar([styled('one', {})], { pending: true })

    const field = await openLetterSpacing(user)
    fireEvent.keyDown(field, { key: 'ArrowDown' })

    await waitFor(() => expect(edits).toEqual([restyled([['one', { chars: { tracking: -10 } }]])]))
    expect(field).toHaveValue('-10')
  })

  it('takes the spacing off whole boxes with Reset', async () => {
    const { edits, user } = await renderBar([styled('one', { tracking: 50 }), styled('two', {})])

    await openLetterSpacing(user)
    await user.click(screen.getByRole('button', { name: 'Reset letter spacing' }))

    expect(edits).toEqual([
      restyled([
        ['one', { chars: { tracking: null } }],
        ['two', { chars: { tracking: null } }],
      ]),
    ])
  })

  it('spaces words chosen in Story through the words edit, Reset giving them 0', async () => {
    const { edits, user } = await renderBar([hello({ tracking: 50 }, { tracking: -20 })])
    act(() => setWordRange(world))

    const field = await openLetterSpacing(user)
    // The words' own spacing.
    expect(field).toHaveValue('-20')
    await user.clear(field)
    await user.type(field, '200{Enter}')
    // Leaving the box for Reset sends the typed value no second time, though the words, which
    // show no draft, haven't taken it yet.
    await user.click(screen.getByRole('button', { name: 'Reset letter spacing' }))
    // A value typed after Reset goes again.
    await user.clear(field)
    await user.type(field, '200{Enter}')

    expect(commands.booharuStyleWords).toHaveBeenCalledTimes(3)
    expect(commands.booharuStyleWords).toHaveBeenNthCalledWith(1, 'text', 6, 11, 'Hello world', {
      set: { tracking: 200 },
    })
    // A mark can't unset a value, so none is 0.
    expect(commands.booharuStyleWords).toHaveBeenNthCalledWith(2, 'text', 6, 11, 'Hello world', {
      set: { tracking: 0 },
    })
    expect(commands.booharuStyleWords).toHaveBeenNthCalledWith(3, 'text', 6, 11, 'Hello world', {
      set: { tracking: 200 },
    })
    await settled()
    expect(edits).toEqual([])
  })

  it('spaces the characters typed, Reset giving them 0', async () => {
    const { edits, user } = await renderBar([hello({})], { typing: typing({ tracking: 30 }) })

    const field = await openLetterSpacing(user)
    expect(field).toHaveValue('30')
    fireEvent.keyDown(field, { key: 'ArrowUp' })
    await user.click(screen.getByRole('button', { name: 'Reset letter spacing' }))

    expect(commands.styleTyping).toHaveBeenCalledTimes(2)
    expect(commands.styleTyping).toHaveBeenNthCalledWith(1, { tracking: 40 })
    expect(commands.styleTyping).toHaveBeenNthCalledWith(2, { tracking: 0 })
    await settled()
    expect(edits).toEqual([])
  })

  it('steps back to the spacing the characters typed show before they take a step', async () => {
    const { user } = await renderBar([hello({})], { typing: typing({ tracking: 30 }) })

    const field = await openLetterSpacing(user)
    fireEvent.keyDown(field, { key: 'ArrowUp' })
    fireEvent.keyDown(field, { key: 'ArrowDown' })

    await waitFor(() => expect(commands.styleTyping).toHaveBeenCalledTimes(2))
    expect(commands.styleTyping).toHaveBeenNthCalledWith(1, { tracking: 40 })
    expect(commands.styleTyping).toHaveBeenNthCalledWith(2, { tracking: 30 })
  })

  it('sends nothing for a cleared or unchanged box, which shows the spacing again', async () => {
    const { edits, user } = await renderBar([styled('one', { tracking: 50 })])

    const field = await openLetterSpacing(user)
    await user.clear(field)
    fireEvent.blur(field)
    expect(field).toHaveValue('50')

    await user.type(field, '{Enter}')
    await settled()
    expect(edits).toEqual([])
  })
})
