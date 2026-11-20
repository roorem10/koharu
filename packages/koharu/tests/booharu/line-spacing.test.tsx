import { fireEvent, screen } from '@testing-library/react'
import type userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { renderBar, restyled, text } from './typeBar'

/** Opens the type bar's line spacing and gives its number box. */
async function openLineSpacing(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button', { name: 'Line spacing' }))
  return screen.findByRole('textbox', { name: 'Line spacing' })
}

describe('Booharu line spacing in the type bar (S11-i)', () => {
  it('shows the spacing the texts share', async () => {
    const { user } = renderBar([
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
    const { user } = renderBar([text('one'), text('two', { style: { line_height: 1.2 } })])

    expect(await openLineSpacing(user)).toHaveValue('1.2')
  })

  it('shows nothing, Mixed, where the spacings differ', async () => {
    const { user } = renderBar([text('one', { style: { line_height: 1.5 } }), text('two')])

    const field = await openLineSpacing(user)
    expect(field).toHaveValue('')
    expect(field).toHaveAttribute('placeholder', 'Mixed')
  })

  it('takes the spacing off every text with Reset, in one step', async () => {
    const { edits, user } = renderBar([text('one', { style: { line_height: 1.5 } }), text('two')])

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
    const { edits, user } = renderBar([text('one', { style: { line_height: 1.5 } }), text('two')], {
      pending: true,
    })

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
    expect(edits).toHaveLength(1)
  })

  it('steps by 0.05 with the arrow keys, each step one edit', async () => {
    const { edits, user } = renderBar([text('one')], { pending: true })

    const field = await openLineSpacing(user)
    fireEvent.keyDown(field, { key: 'ArrowUp' })

    expect(edits).toEqual([restyled([['one', { line_height: 1.25 }]])])
    expect(field).toHaveValue('1.25')
  })

  it('sends nothing for a cleared or unchanged box, which shows the spacing again', async () => {
    const { edits, user } = renderBar([text('one', { style: { line_height: 1.5 } })])

    const field = await openLineSpacing(user)
    await user.clear(field)
    fireEvent.blur(field)
    expect(field).toHaveValue('1.5')

    await user.type(field, '{Enter}')
    expect(edits).toEqual([])
  })

  it('is disabled for a new text being typed, which has no paragraph yet', () => {
    renderBar([], { typing: { layer: null, chars: {}, mixed: [] }, selected: [] })

    expect(screen.getByRole('button', { name: 'Line spacing' })).toBeDisabled()
  })
})
