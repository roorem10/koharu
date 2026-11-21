import { act, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { setWordRange, useWordRange, type WordRange } from '@/lib/booharu/words'
import { commands, type Layer, type Typed } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { renderBar, settled, text } from './typeBar'

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

const SELECTED = "Clear the selected words' styles"

describe('Booharu Applies to in the type bar (S11-f)', () => {
  it('quotes the characters selected while typing, first, and Whole box keeps what was typed', async () => {
    const { bar, user, edits } = await renderBar([hello()], { typing: typing('Hello wo') })

    expect(screen.getByTestId('type-bar').firstElementChild).toHaveTextContent(
      /^Applies to "Hello wo"Whole box$/,
    )
    // The bar cuts the quote to its room, so a tooltip has it whole.
    await user.hover(bar.getByText('Applies to "Hello wo"'))
    expect(await tooltip('Applies to "Hello wo"')).toBeInTheDocument()

    const wholeBox = bar.getByRole('button', { name: 'Whole box' })
    await user.hover(wholeBox)
    expect(await tooltip('Style the whole box instead')).toBeInTheDocument()
    await user.click(wholeBox)

    expect(commands.booharuFinishTyping).toHaveBeenCalledOnce()
    await settled()
    expect(commands.styleTyping).not.toHaveBeenCalled()
    expect(edits).toEqual([])
  })

  it('quotes the words chosen in Story, and Whole box lets them go', async () => {
    const { bar, user } = await renderBar([hello()])
    act(() => setWordRange(world))

    expect(bar.getByText('Applies to "world"')).toBeInTheDocument()
    await user.click(bar.getByRole('button', { name: 'Whole box' }))

    expect(useWordRange.getState().range).toBeNull()
    expect(bar.queryByText(/^Applies to/)).toBeNull()
    await settled()
    expect(commands.booharuFinishTyping).not.toHaveBeenCalled()
  })

  it.each([
    ['at a caret while typing', typing()],
    ['at a caret the canvas quotes as none', typing(null)],
    ['with nothing chosen', null],
  ])('shows nothing %s', async (_, typed) => {
    const { bar } = await renderBar([hello()], { typing: typed })

    expect(bar.queryByText(/^Applies to/)).toBeNull()
    expect(bar.queryByRole('button', { name: 'Whole box' })).toBeNull()
  })

  it("says the paragraph settings are the whole box's while typing, greying none", async () => {
    const { bar, user } = await renderBar([hello()], { typing: typing() })

    const center = bar.getByRole('button', { name: 'Align center' })
    expect(center).toBeEnabled()
    await user.hover(center)
    expect(await tooltip('Align center')).toHaveTextContent(
      /^Align centerApplies to the whole box$/,
    )

    const vertical = bar.getByRole('button', { name: 'Vertical' })
    expect(vertical).toBeEnabled()
    await user.hover(vertical)
    expect(await tooltip('Text direction')).toHaveTextContent(
      /^Text directionHorizontalApplies to the whole box$/,
    )

    // Line spacing sits in Spacing, beside letter spacing, which goes to the characters typed
    // (S12-k).
    await user.click(bar.getByRole('button', { name: 'Spacing' }))
    expect(await screen.findByRole('textbox', { name: 'Line spacing' })).toBeEnabled()
    expect(screen.getByText('Line spacing')).toHaveTextContent(
      /^Line spacingApplies to the whole box$/,
    )
    await user.keyboard('{Escape}')

    // The outline method sits in More border options, beside Koharu's border colour and width.
    await user.click(bar.getByRole('button', { name: 'More border options' }))
    expect(await screen.findByText('Outline method')).toHaveTextContent(
      /^Outline methodApplies to the whole box$/,
    )
    expect(screen.getByRole('combobox', { name: 'Outline method' })).toBeEnabled()
  })

  it('says so with words chosen in Story too, and nothing of the kind on a whole box', async () => {
    const { bar, user } = await renderBar([hello()])

    await user.hover(bar.getByRole('button', { name: 'Vertical' }))
    expect(await tooltip('Text direction')).toHaveTextContent(/^Text directionHorizontal$/)

    act(() => setWordRange(world))
    await user.hover(bar.getByRole('button', { name: 'Align center' }))
    expect(await tooltip('Align center')).toHaveTextContent(
      /^Align centerApplies to the whole box$/,
    )
  })
})

describe('Booharu A− and A+ in the type bar (S11-e)', () => {
  it('steps the characters typed, one call a click, after the size field, never the fit', async () => {
    const { bar, user, edits } = await renderBar([hello()], { typing: typing('Hello') })

    const smaller = bar.getByRole('button', { name: 'Smaller' })
    const bigger = bar.getByRole('button', { name: 'Bigger' })
    expect(smaller).toHaveTextContent('A−')
    expect(bigger).toHaveTextContent('A+')
    const buttons = bar.getAllByRole('button')
    expect(buttons.indexOf(smaller)).toBe(
      buttons.indexOf(bar.getByRole('button', { name: 'Choose font size' })) + 1,
    )
    await user.hover(bigger)
    expect(await tooltip('Bigger')).toBeInTheDocument()

    await user.click(bigger)
    await user.click(smaller)
    await user.click(bigger)

    expect(vi.mocked(commands.booharuStepTypingSize).mock.calls).toEqual([[true], [false], [true]])
    await settled()
    expect(commands.styleTyping).not.toHaveBeenCalled()
    expect(edits).toEqual([])
  })

  it('steps the next characters typed at a caret', async () => {
    const { bar, user } = await renderBar([hello()], { typing: typing() })

    await user.click(bar.getByRole('button', { name: 'Smaller' }))

    expect(commands.booharuStepTypingSize).toHaveBeenCalledExactlyOnceWith(false)
  })

  it('steps the words chosen in Story through the words edit', async () => {
    const { bar, user, edits } = await renderBar([hello()])
    act(() => setWordRange(world))

    await user.click(bar.getByRole('button', { name: 'Smaller' }))
    expect(commands.booharuStyleWords).toHaveBeenCalledExactlyOnceWith(
      'text',
      6,
      11,
      'Hello world',
      {
        step: 'smaller',
      },
    )
    await user.click(bar.getByRole('button', { name: 'Bigger' }))
    expect(commands.booharuStyleWords).toHaveBeenLastCalledWith('text', 6, 11, 'Hello world', {
      step: 'bigger',
    })

    await settled()
    expect(commands.booharuStepTypingSize).not.toHaveBeenCalled()
    expect(edits).toEqual([])
  })

  it('shows neither on a whole box', async () => {
    const { bar } = await renderBar([hello(), hello('two')])

    expect(bar.queryByRole('button', { name: 'Smaller' })).toBeNull()
    expect(bar.queryByRole('button', { name: 'Bigger' })).toBeNull()
  })
})

describe('Booharu Clear in the type bar (S11-g)', () => {
  it('takes the styles off the characters selected while typing, after the star', async () => {
    const { bar, user, edits } = await renderBar([hello('text', true)], { typing: typing('world') })

    const clear = bar.getByRole('button', { name: SELECTED })
    expect(bar.getByRole('button', { name: 'Emphasis' }).nextElementSibling).toBe(clear)
    await user.hover(clear)
    expect(await tooltip(SELECTED)).toBeInTheDocument()
    await user.click(clear)

    expect(commands.booharuUnmarkTyping).toHaveBeenCalledExactlyOnceWith('all')
    await settled()
    expect(commands.booharuClearWordStyles).not.toHaveBeenCalled()
    expect(edits).toEqual([])
  })

  it('is disabled at a caret while typing, a text with word styles too', async () => {
    const { bar } = await renderBar([hello('text', true)], { typing: typing() })

    expect(bar.getByRole('button', { name: SELECTED })).toBeDisabled()
  })

  it('takes the styles off the words chosen in Story', async () => {
    const { bar, user } = await renderBar([hello()])
    act(() => setWordRange(world))

    await user.click(bar.getByRole('button', { name: SELECTED }))

    expect(commands.booharuStyleWords).toHaveBeenCalledExactlyOnceWith(
      'text',
      6,
      11,
      'Hello world',
      {
        clear: true,
      },
    )
    await settled()
    expect(commands.booharuClearWordStyles).not.toHaveBeenCalled()
  })

  it('takes every word style off the selected boxes in one step', async () => {
    const { bar, user, edits } = await renderBar([hello('one', true), hello('two')])
    const shown = vi.spyOn(toast, 'add')

    const clear = bar.getByRole('button', { name: 'Clear all word styles in the 2 selected boxes' })
    await user.hover(clear)
    expect(await tooltip('Clear all word styles in the 2 selected boxes')).toBeInTheDocument()
    await user.click(clear)

    expect(commands.booharuClearWordStyles).toHaveBeenCalledExactlyOnceWith(['one', 'two'])
    await settled()
    expect(edits).toEqual([])
    expect(shown).not.toHaveBeenCalled()
  })

  it('names the one box, and counts the locked boxes it left as they are', async () => {
    const { bar, user } = await renderBar([hello('one', true)])
    vi.mocked(commands.booharuClearWordStyles).mockResolvedValue({ cleared: 0, locked: 1 })
    const shown = vi.spyOn(toast, 'add')

    await user.click(bar.getByRole('button', { name: 'Clear all word styles in this box' }))

    expect(commands.booharuClearWordStyles).toHaveBeenCalledExactlyOnceWith(['one'])
    await vi.waitFor(() =>
      expect(shown).toHaveBeenCalledExactlyOnceWith({
        type: 'info',
        title: '1 locked box was left as it is',
      }),
    )
  })

  it('counts several locked boxes', async () => {
    const { bar, user } = await renderBar([
      hello('one', true),
      hello('two', true),
      hello('three', true),
    ])
    vi.mocked(commands.booharuClearWordStyles).mockResolvedValue({ cleared: 1, locked: 2 })
    const shown = vi.spyOn(toast, 'add')

    await user.click(
      bar.getByRole('button', { name: 'Clear all word styles in the 3 selected boxes' }),
    )

    await vi.waitFor(() =>
      expect(shown).toHaveBeenCalledExactlyOnceWith({
        type: 'info',
        title: '2 locked boxes were left as they are',
      }),
    )
  })

  it('is disabled while no selected box has a word style', async () => {
    const { bar } = await renderBar([hello('one'), hello('two')])

    expect(
      bar.getByRole('button', { name: 'Clear all word styles in the 2 selected boxes' }),
    ).toBeDisabled()
  })
})

describe('Booharu Done in the type bar (S11-c)', () => {
  it('keeps what was typed, last in the bar, only while typing', async () => {
    const { bar, user, edits } = await renderBar([hello()], { typing: typing() })

    const done = bar.getByRole('button', { name: 'Done' })
    expect(bar.getAllByRole('button').at(-1)).toBe(done)
    await user.hover(done)
    expect(await tooltip('Keep what you typed (Esc)')).toBeInTheDocument()
    await user.click(done)

    expect(commands.booharuFinishTyping).toHaveBeenCalledOnce()
    await settled()
    expect(edits).toEqual([])
  })

  it('is not there on a whole box, or with words chosen in Story', async () => {
    const { bar } = await renderBar([hello()])
    expect(bar.queryByRole('button', { name: 'Done' })).toBeNull()

    act(() => setWordRange(world))
    expect(bar.queryByRole('button', { name: 'Done' })).toBeNull()
  })
})
