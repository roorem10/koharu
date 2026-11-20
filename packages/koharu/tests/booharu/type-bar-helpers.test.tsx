import { act, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { setWordRange, type WordRange } from '@/lib/booharu/words'
import { commands, type Layer, type Typed } from '@koharu/ipc/protocol'

import { renderBar, text } from './typeBar'

/** The canvas typing into the text `text`: the characters `selected` quotes chosen, or a caret
 * without them. */
function typing(selected?: string | null): Typed {
  return { layer: 'text', chars: {}, mixed: [], ...(selected === undefined ? {} : { selected }) }
}

/** A text layer `id` reading "Hello world", its "world" bold when `marked`. */
function hello(id = 'text', marked = false): Layer {
  return text(id, {
    content: marked
      ? [{ insert: 'Hello ' }, { insert: 'world', attributes: { weight: 700 } }]
      : [{ insert: 'Hello world' }],
  })
}

/** "world", chosen in the Story panel in `hello`. */
const world: WordRange = { layer: 'text', start: 6, end: 11, text: 'Hello world' }

/** The tooltip whose own text is `label`. */
const tooltip = (label: string) =>
  screen.findByText(label, { selector: '[data-slot=tooltip-content]' })

describe('Booharu Done in the type bar (S11-c)', () => {
  it('keeps what was typed, last in the bar, only while typing', async () => {
    const { bar, user, edits } = renderBar([hello()], { typing: typing() })

    const done = bar.getByRole('button', { name: 'Done' })
    expect(bar.getAllByRole('button').at(-1)).toBe(done)
    await user.hover(done)
    expect(await tooltip('Keep what you typed (Esc)')).toBeInTheDocument()
    await user.click(done)

    expect(commands.booharuFinishTyping).toHaveBeenCalledOnce()
    expect(edits).toEqual([])
  })

  it('is not there on a whole box, or with words chosen in Story', () => {
    const { bar } = renderBar([hello()])
    expect(bar.queryByRole('button', { name: 'Done' })).toBeNull()

    act(() => setWordRange(world))
    expect(bar.queryByRole('button', { name: 'Done' })).toBeNull()
  })
})
