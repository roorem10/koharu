import { screen } from '@testing-library/react'
import type userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import type { CharStyle, StrokeMethod } from '@koharu/ipc/protocol'

import { renderBar, restyled, text } from './typeBar'

// Booharu's border method in the type bar's Border menu: Koharu's border, Expansion, is the
// default, what a text with no method draws; Contour is the choice beside it.

const WHITE: CharStyle['stroke'] = [1, 1, 1, 1]

/** A text `id` bordered in white, its borders drawn by `method`, or with no method. */
function bordered(id: string, method?: StrokeMethod) {
  return text(id, {
    style: { chars: { stroke: WHITE, stroke_width: 2 }, ...(method && { stroke_method: method }) },
  })
}

/** Opens the Border menu, and its outline methods once the one shown is `shown`. */
async function openMethods(user: ReturnType<typeof userEvent.setup>, shown: string) {
  await user.click(screen.getByRole('button', { name: 'Border' }))
  const method = await screen.findByRole('combobox', { name: 'Outline method' })
  expect(method).toHaveTextContent(shown)
  await user.click(method)
}

describe('Booharu border method in the type bar', () => {
  it('shows Expansion, listed first, for a text with no method', async () => {
    const { user } = renderBar([bordered('one')])

    await openMethods(user, 'Expansion')
    const options = await screen.findAllByRole('option')
    expect(options.map((option) => option.textContent)).toEqual(['Expansion', 'Contour stroke'])
  })

  it('writes contour when Contour is picked', async () => {
    const { edits, user } = renderBar([bordered('one')])

    await openMethods(user, 'Expansion')
    await user.click(await screen.findByRole('option', { name: 'Contour stroke' }))

    expect(edits).toEqual([restyled([['one', { stroke_method: 'contour' }]])])
  })

  it('writes no method when Expansion is picked, so Koharu reads the text', async () => {
    const { edits, user } = renderBar([bordered('one', 'contour')])

    await openMethods(user, 'Contour stroke')
    await user.click(await screen.findByRole('option', { name: 'Expansion' }))

    expect(edits).toEqual([restyled([['one', { stroke_method: null }]])])
  })
})
