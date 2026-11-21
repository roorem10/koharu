import { act, fireEvent, screen, waitFor } from '@testing-library/react'
import type userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { setWordRange, type WordRange } from '@/lib/booharu/words'
import { commands, type CharStyle, type Gradient, type Rgba } from '@koharu/ipc/protocol'

import { renderBar, restyled, settled, styled, text } from './typeBar'

const RED: Rgba = [1, 0, 0, 1]
const GREEN: Rgba = [0, 1, 0, 1]
const BLUE: Rgba = [0, 0, 1, 1]

const GRADIENT: Gradient = {
  kind: 'linear',
  angle: 90,
  stops: [
    { offset: 0, color: RED },
    { offset: 1, color: BLUE },
  ],
}

/** "world", chosen in the Story panel in the text `text` reading "Hello world". */
const world: WordRange = { layer: 'text', start: 6, end: 11, text: 'Hello world' }

/** Opens the text colour well and answers the hex code it shows, empty for none. */
async function openTextColor(user: ReturnType<typeof userEvent.setup>): Promise<string> {
  await user.click(screen.getByRole('button', { name: 'Text color' }))
  const hex = await screen.findByRole('textbox', { name: 'Hex color code' })
  // The field puts its # before no colour too.
  return (hex as HTMLInputElement).value.replace(/^#$/, '')
}

/** Types `hex` into the open colour picker, which picks it. */
function pickColor(hex: string) {
  fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
    target: { value: hex },
  })
}

/** A text layer `id` reading "Hello world", styled `chars`, "world" marked `marks` when given. */
function hello(chars: CharStyle, marks?: CharStyle) {
  return text('text', {
    style: { chars },
    content: marks
      ? [{ insert: 'Hello ' }, { insert: 'world', attributes: marks }]
      : [{ insert: 'Hello world' }],
  })
}

describe('Booharu text colour over a gradient fill (S12-c)', () => {
  it('shows none over a gradient box, and a colour picked takes the gradient off', async () => {
    const { edits, user } = await renderBar(
      [styled('box', { fill: RED, fill_gradient: GRADIENT })],
      {
        pending: true,
      },
    )

    expect(await openTextColor(user)).toBe('')
    pickColor('#00FF00')

    // The edit goes as a mutation, a turn after the pick.
    await waitFor(() =>
      expect(edits).toEqual([restyled([['box', { chars: { fill: GREEN, fill_gradient: null } }]])]),
    )
    // The colour shows at once, the gradient gone, so it can swap with the border's.
    await user.keyboard('{Escape}')
    await user.click(screen.getByRole('button', { name: 'More border options' }))
    expect(
      await screen.findByRole('button', { name: 'Swap fill and outline colors' }),
    ).toBeEnabled()
  })

  it("takes the words' own paint off a box whose words show a gradient", async () => {
    const { edits, user } = await renderBar([hello({ fill: RED }, { fill_gradient: GRADIENT })])

    // The box shows two paints.
    expect(await openTextColor(user)).toBe('')
    pickColor('#0000FF')

    await waitFor(() => expect(edits).toHaveLength(1))
    expect(edits).toEqual([
      {
        label: 'set_style',
        ops: [
          {
            op: 'update_layer',
            layer: 'text',
            patch: { style: { chars: { fill: BLUE, fill_gradient: null } } },
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

  it('shows the colour of a word given one in a gradient box, as the words edit gives it', async () => {
    const { edits, user } = await renderBar([hello({ fill_gradient: GRADIENT }, { fill: RED })])
    act(() => setWordRange(world))

    expect(await openTextColor(user)).toBe('#FF0000')
    pickColor('#00FF00')

    expect(commands.booharuStyleWords).toHaveBeenCalledExactlyOnceWith(
      'text',
      6,
      11,
      'Hello world',
      { set: { fill: GREEN, fill_gradient: null } },
    )
    await settled()
    expect(edits).toEqual([])
  })

  it('shows none for chosen words showing the gradient of their box', async () => {
    const { user } = await renderBar([hello({ fill: RED, fill_gradient: GRADIENT })])
    act(() => setWordRange(world))

    expect(await openTextColor(user)).toBe('')
  })

  it('shows none while typing characters showing a gradient, and a colour picked styles them', async () => {
    const { edits, user } = await renderBar([hello({})], {
      typing: { layer: 'text', chars: { fill: RED, fill_gradient: GRADIENT }, mixed: [] },
    })

    expect(await openTextColor(user)).toBe('')
    pickColor('#00FF00')

    // The canvas styles them a turn after the pick.
    await waitFor(() =>
      expect(commands.styleTyping).toHaveBeenCalledExactlyOnceWith({
        fill: GREEN,
        fill_gradient: null,
      }),
    )
    await settled()
    expect(edits).toEqual([])
  })

  it('shows the colour typed characters share with no gradient', async () => {
    const { user } = await renderBar([hello({})], {
      typing: { layer: 'text', chars: { fill: RED }, mixed: [] },
    })

    expect(await openTextColor(user)).toBe('#FF0000')
  })

  it.each([
    ['shows a gradient', styled('box', { fill: RED, fill_gradient: GRADIENT })],
    ['shows one in some characters', hello({ fill: RED }, { fill_gradient: GRADIENT })],
  ])('offers no colour swap while the text %s', async (_, layer) => {
    const { user } = await renderBar([layer])

    await user.click(screen.getByRole('button', { name: 'More border options' }))

    expect(
      await screen.findByRole('button', { name: 'Swap fill and outline colors' }),
    ).toBeDisabled()
  })

  it('swaps a flat colour as before', async () => {
    const { edits, user } = await renderBar([styled('box', { fill: RED })])

    await user.click(screen.getByRole('button', { name: 'More border options' }))
    await user.click(await screen.findByRole('button', { name: 'Swap fill and outline colors' }))

    await waitFor(() => expect(edits).toHaveLength(1))
    expect(edits).toEqual([
      restyled([
        [
          'box',
          { chars: { fill: [1, 1, 1, 1], fill_gradient: null, stroke: RED, stroke_width: 1.5 } },
        ],
      ]),
    ])
  })

  it("keeps a box gradient its words' own colours hide off as it swaps", async () => {
    const hidden = text('text', {
      style: { chars: { fill_gradient: GRADIENT } },
      content: [{ insert: 'Hello world', attributes: { fill: RED } }],
    })
    const { edits, user } = await renderBar([hidden])

    await user.click(screen.getByRole('button', { name: 'More border options' }))
    await user.click(await screen.findByRole('button', { name: 'Swap fill and outline colors' }))

    // The words' colour goes to the box with its gradient off, which would show again otherwise.
    await waitFor(() => expect(edits).toHaveLength(1))
    expect(edits).toEqual([
      {
        label: 'set_style',
        ops: [
          {
            op: 'update_layer',
            layer: 'text',
            patch: {
              style: {
                chars: { fill: [1, 1, 1, 1], fill_gradient: null, stroke: RED, stroke_width: 1.5 },
              },
            },
          },
          {
            op: 'set_rich_text',
            layer: 'text',
            spans: [{ insert: 'Hello world', attributes: {} }],
          },
        ],
      },
    ])
  })
})
