import { screen, waitFor } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { commands, type Edit, type Fit, type PathFit } from '@koharu/ipc/protocol'

import { renderBar, restyled, settled, text } from './typeBar'

// Koharu's size field in the Booharu type bar: a size typed or picked is the size the text draws
// at, as in Photoshop, from 0.5 to 1296 pixels, so a box that shrinks or fits its text by the best
// size stops fitting it, and a growing box keeps its fit, each text in its own frame (#1499).
// Booharu adds "Mixed" while the boxes' sizes differ, or only some of their frames fit by Auto
// (S11-h), and A− and A+ (S11-e), which step each character from its own size and never touch the
// fit.

/** A box frame fitting its text by `fit`. */
const box = (fit: Fit) => ({ box: { bounds: { x: 0, y: 0, width: 100, height: 50 }, fit } })

/** A path frame fitting its text by `fit`, as a balloon's translation fills its shape. */
const path = (fit: PathFit) => ({ path: { path: 'M0 0H100V50H0Z', fit } })

/** The edit Koharu sends to fit the text of each of `layers`, boxes, by `fit`: one edit for them
 * all, a growing box left out of it when its fitting goes off (#1499). */
const refit = (fit: Fit, ...layers: string[]): Edit => ({
  label: 'update_layer',
  ops: layers.map((layer) => ({ op: 'update_layer', layer, patch: { frame: { box: { fit } } } })),
})

describe('Booharu font size', () => {
  it('applies a typed size as one edit when Enter is pressed', async () => {
    const { edits, user } = await renderBar([text('text')])

    const size = screen.getByTestId('type-size')
    await user.clear(size)
    await user.type(size, '8')
    await settled()
    expect(edits).toEqual([])
    await user.keyboard('{Enter}')

    expect(size).not.toHaveFocus()
    await waitFor(() => expect(edits).toEqual([restyled([['text', { chars: { size: 8 } }]])]))
  })

  it('sets a typed size exactly, off the A− and A+ steps’ half-pixel grain, and turns an Auto box’s fitting off', async () => {
    const { edits, user } = await renderBar([text('one', { frame: box('best') })])

    const size = screen.getByTestId('type-size')
    expect(size).toHaveAttribute('placeholder', 'Auto')
    await user.type(size, '412.3{Enter}')

    await waitFor(() => expect(edits).toHaveLength(2))
    expect(edits).toEqual(
      expect.arrayContaining([
        restyled([['one', { chars: { size: 412.3 } }]]),
        refit('none', 'one'),
      ]),
    )
  })

  it('turns a shrinking box’s fitting off for a size picked from the menu', async () => {
    const { edits, user } = await renderBar([text('one', { frame: box('shrink') })])

    await user.click(screen.getByRole('button', { name: 'Choose font size' }))
    await user.click(await screen.findByRole('menuitemradio', { name: '36' }))

    await waitFor(() => expect(edits).toHaveLength(2))
    expect(edits).toEqual(
      expect.arrayContaining([restyled([['one', { chars: { size: 36 } }]]), refit('none', 'one')]),
    )
  })

  it('keeps a growing box’s fit, which keeps the size', async () => {
    const { edits, user } = await renderBar([text('one', { frame: box('grow') })])

    const size = screen.getByTestId('type-size')
    await user.clear(size)
    await user.type(size, '30{Enter}')

    await waitFor(() => expect(edits).toHaveLength(2))
    await settled()
    // Koharu's fit edit leaves the growing box out, so it holds no op.
    expect(edits).toEqual(
      expect.arrayContaining([restyled([['one', { chars: { size: 30 } }]]), refit('none')]),
    )
    expect(edits).toHaveLength(2)
  })

  it('sizes every box while their sizes and fits differ, shown as Mixed, and turns their fitting off', async () => {
    const { edits, user } = await renderBar([
      text('one', { frame: box('best'), style: { chars: { size: 20 } } }),
      text('two', { frame: box('shrink'), style: { chars: { size: 30 } } }),
    ])

    const size = screen.getByTestId('type-size')
    expect(size).toHaveValue('')
    expect(size).toHaveAttribute('placeholder', 'Mixed')
    await user.type(size, '16{Enter}')

    await waitFor(() => expect(edits).toHaveLength(2))
    expect(edits).toEqual(
      expect.arrayContaining([
        restyled([
          ['one', { chars: { size: 16 } }],
          ['two', { chars: { size: 16 } }],
        ]),
        refit('none', 'one', 'two'),
      ]),
    )
  })

  it('shows Mixed while only some frames fit by Auto, their sizes alike, and sizes each in its own frame', async () => {
    const { edits, user } = await renderBar([
      text('balloon', { frame: path('best') }),
      text('sign', { frame: box('shrink') }),
    ])

    // Both 24 high, but frames that don't share Auto share no size (#1499).
    const size = screen.getByTestId('type-size')
    expect(size).toHaveValue('')
    expect(size).toHaveAttribute('placeholder', 'Mixed')
    await user.click(screen.getByRole('button', { name: 'Choose font size' }))
    // Neither Auto nor the size they hold is chosen in the menu.
    const auto = await screen.findByRole('menuitemradio', { name: 'Auto' })
    expect(auto).toHaveAttribute('aria-checked', 'false')
    expect(screen.getByRole('menuitemradio', { name: '24' })).toHaveAttribute(
      'aria-checked',
      'false',
    )
    await user.click(screen.getByRole('menuitemradio', { name: '24' }))

    await waitFor(() => expect(edits).toHaveLength(2))
    expect(edits).toEqual(
      expect.arrayContaining([
        restyled([
          ['balloon', { chars: { size: 24 } }],
          ['sign', { chars: { size: 24 } }],
        ]),
        // One edit stops each frame fitting, the path as a path and the box as a box.
        {
          label: 'update_layer',
          ops: [
            { op: 'update_layer', layer: 'balloon', patch: { frame: { path: { fit: 'none' } } } },
            { op: 'update_layer', layer: 'sign', patch: { frame: { box: { fit: 'none' } } } },
          ],
        },
      ]),
    )
  })

  it('fits every box by the best size again with Auto, setting no size, while their fits differ', async () => {
    const { edits, user } = await renderBar([
      text('one', { frame: box('none') }),
      text('two', { frame: box('best') }),
    ])

    await user.click(screen.getByRole('button', { name: 'Choose font size' }))
    await user.click(await screen.findByRole('menuitemradio', { name: 'Auto' }))

    await waitFor(() => expect(edits).toHaveLength(1))
    await settled()
    expect(edits).toEqual([refit('best', 'one', 'two')])
  })

  it('steps the characters typed with A− and A+, never touching the fit (S11-e)', async () => {
    const { edits, user } = await renderBar([text('one', { frame: box('best') })], {
      typing: { layer: 'one', chars: { size: 24 }, mixed: [] },
    })

    await user.click(screen.getByRole('button', { name: 'Bigger' }))
    await user.click(screen.getByRole('button', { name: 'Smaller' }))

    await waitFor(() => expect(commands.booharuStepTypingSize).toHaveBeenCalledTimes(2))
    expect(commands.booharuStepTypingSize).toHaveBeenNthCalledWith(1, true)
    expect(commands.booharuStepTypingSize).toHaveBeenNthCalledWith(2, false)
    await settled()
    expect(edits).toEqual([])
  })
})
