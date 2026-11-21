import { act, fireEvent, screen, waitFor } from '@testing-library/react'
import type userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import type { CharStyle, StrokeMethod } from '@koharu/ipc/protocol'

import { renderBar, restyled, settled, text } from './typeBar'

// Booharu's More border options in the type bar, a small button beside Koharu's border colour and
// width, holding what Booharu adds to Koharu's border: the 2nd border, the colour swap and the
// border method, where Koharu's border, Expansion, is the default, what a text with no method
// draws, and Contour the choice beside it.

const WHITE: CharStyle['stroke'] = [1, 1, 1, 1]
const BLACK: CharStyle['stroke'] = [0, 0, 0, 1]
const RED: CharStyle['stroke'] = [1, 0, 0, 1]

/** A text `id` bordered in white, its borders drawn by `method`, or with no method. */
function bordered(id: string, method?: StrokeMethod) {
  return text(id, {
    style: { chars: { stroke: WHITE, stroke_width: 2 }, ...(method && { stroke_method: method }) },
  })
}

/** Types `hex` into the open colour picker, which picks it. */
function pickColor(hex: string) {
  fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
    target: { value: hex },
  })
}

/** Opens More border options. */
async function openExtras(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button', { name: 'More border options' }))
}

/** Opens More border options, and its outline methods once the one shown is `shown`. */
async function openMethods(user: ReturnType<typeof userEvent.setup>, shown: string) {
  await openExtras(user)
  const method = await screen.findByRole('combobox', { name: 'Outline method' })
  expect(method).toHaveTextContent(shown)
  await user.click(method)
}

describe('Booharu border method in the type bar', () => {
  it('shows Expansion, listed first, for a text with no method', async () => {
    const { user } = await renderBar([bordered('one')])

    await openMethods(user, 'Expansion')
    const options = await screen.findAllByRole('option')
    expect(options.map((option) => option.textContent)).toEqual(['Expansion', 'Contour stroke'])
  })

  it('writes contour when Contour is picked', async () => {
    const { edits, user } = await renderBar([bordered('one')])

    await openMethods(user, 'Expansion')
    await user.click(await screen.findByRole('option', { name: 'Contour stroke' }))

    await waitFor(() => expect(edits).toEqual([restyled([['one', { stroke_method: 'contour' }]])]))
  })

  it('writes no method when Expansion is picked, so Koharu reads the text', async () => {
    const { edits, user } = await renderBar([bordered('one', 'contour')])

    await openMethods(user, 'Contour stroke')
    await user.click(await screen.findByRole('option', { name: 'Expansion' }))

    await waitFor(() => expect(edits).toEqual([restyled([['one', { stroke_method: null }]])]))
  })
})

describe("Booharu's More border options in the type bar", () => {
  it('is a named button in the border group, which Tab reaches after the width and Enter opens', async () => {
    const { bar, user } = await renderBar([bordered('one')])

    const more = bar.getByRole('button', { name: 'More border options' })
    expect(bar.getByRole('group', { name: 'Border' })).toContainElement(more)
    act(() => bar.getByRole('button', { name: 'Choose border width' }).focus())
    await user.tab()
    expect(more).toHaveFocus()
    await user.keyboard('{Enter}')

    expect(await screen.findByRole('button', { name: 'Second border color' })).toBeEnabled()
    expect(screen.getByRole('textbox', { name: 'Second border width' })).toBeEnabled()
    expect(screen.getByRole('button', { name: 'Swap fill and outline colors' })).toBeEnabled()
    expect(screen.getByRole('combobox', { name: 'Outline method' })).toHaveTextContent('Expansion')
  })

  it('says what it holds in its tooltip', async () => {
    const { bar, user } = await renderBar([bordered('one')])

    await user.hover(bar.getByRole('button', { name: 'More border options' }))
    expect(
      await screen.findByText('More border options', {
        selector: '[data-slot=tooltip-content]',
      }),
    ).toBeInTheDocument()
  })

  it('gives a bordered text a second border, black unless picked, at the width set', async () => {
    const { edits, user } = await renderBar([bordered('one')])

    await openExtras(user)
    const width = await screen.findByRole('textbox', { name: 'Second border width' })
    expect(width).toHaveValue('3')
    await user.clear(width)
    await user.type(width, '5')
    // A typed width waits for Enter, as Koharu 0.90's border width does.
    await settled()
    expect(edits).toEqual([])
    await user.keyboard('{Enter}')

    await waitFor(() =>
      expect(edits).toEqual([
        restyled([['one', { chars: { outer_stroke_width: 5, outer_stroke: BLACK } }]]),
      ]),
    )
    expect(width).not.toHaveFocus()
  })

  it('sends no second border for an emptied width box, which shows the width again', async () => {
    const { edits, user } = await renderBar([bordered('one')])

    await openExtras(user)
    const width = await screen.findByRole('textbox', { name: 'Second border width' })
    await user.clear(width)
    await user.keyboard('{Enter}')

    await settled()
    expect(edits).toEqual([])
    expect(width).toHaveValue('3')
  })

  it('gives a bordered text a second border in the colour picked, 3 wide unless set', async () => {
    const { edits, user } = await renderBar([bordered('one')])

    await openExtras(user)
    await user.click(await screen.findByRole('button', { name: 'Second border color' }))
    pickColor('#FF0000')

    await waitFor(() =>
      expect(edits).toEqual([
        restyled([['one', { chars: { outer_stroke: RED, outer_stroke_width: 3 } }]]),
      ]),
    )
  })

  it('takes the second border off with Transparent, its colour and width both', async () => {
    const { edits, user } = await renderBar([
      text('one', {
        style: {
          chars: { stroke: WHITE, stroke_width: 2, outer_stroke: RED, outer_stroke_width: 4 },
        },
      }),
    ])

    await openExtras(user)
    await user.click(await screen.findByRole('button', { name: 'Second border color' }))
    await user.click(await screen.findByRole('button', { name: 'Transparent' }))

    await waitFor(() =>
      expect(edits).toEqual([
        restyled([['one', { chars: { outer_stroke: null, outer_stroke_width: null } }]]),
      ]),
    )
  })

  it('offers no second border on a text with no first border', async () => {
    const { user } = await renderBar([text('one')])

    await openExtras(user)

    expect(await screen.findByRole('button', { name: 'Second border color' })).toBeDisabled()
    expect(screen.getByRole('textbox', { name: 'Second border width' })).toBeDisabled()
  })

  it('offers no colour swap while the border widths differ', async () => {
    const { user } = await renderBar([
      text('one', { style: { chars: { stroke: WHITE, stroke_width: 2 } } }),
      text('two', { style: { chars: { stroke: WHITE, stroke_width: 4 } } }),
    ])

    await openExtras(user)

    expect(
      await screen.findByRole('button', { name: 'Swap fill and outline colors' }),
    ).toBeDisabled()
  })
})
