import { fireEvent, screen, waitFor, within } from '@testing-library/react'
import type userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { effectsKey } from '@/lib/booharu/effects'
import { queryClient } from '@/lib/queries'
import { commands, type SkewCounts, type TextEffects, type Typed } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { block } from './boxes'
import { renderBar, styled, text } from './typeBar'

type User = ReturnType<typeof userEvent.setup>

/** The page's text effects as the desktop reads them: each of `skews`' boxes skewed as it says. */
function skewed(skews: Record<string, { x: number; y: number }>) {
  const effects: TextEffects[] = Object.entries(skews).map(([layer, skew]) => ({
    layer,
    effects: ['skew'],
    skew,
  }))
  queryClient.setQueryData(effectsKey('page'), effects)
}

/** `booharu_set_skew` answering `counts`. */
function answers(counts: Partial<SkewCounts>) {
  vi.mocked(commands.booharuSetSkew).mockResolvedValue({
    changed: 0,
    locked: 0,
    untranslated: 0,
    ...counts,
  })
}

/** Opens Text effects from the type bar's fx button and gives its Skew section. */
async function openSkew(user: User) {
  await user.click(screen.getByRole('button', { name: 'Text effects' }))
  const effects = within(await screen.findByRole('dialog', { name: 'Text effects' }))
  return within(effects.getByRole('group', { name: 'Skew' }))
}

/** The canvas typing in `layer`, a new text for none. */
function typing(layer: string | null = 'one'): Typed {
  return { layer, chars: {}, mixed: [] } as Typed
}

describe('Booharu Skew in Text effects (S12-k)', () => {
  it('shows the skew the boxes share along each axis, and Mixed where they differ', async () => {
    skewed({ one: { x: 20, y: -5 }, two: { x: 20, y: 10 } })
    const { user } = renderBar([styled('one', {}), styled('two', {})])

    const skew = await openSkew(user)

    const horizontal = skew.getByRole('textbox', { name: 'Horizontal (°)' })
    expect(horizontal).toHaveValue('20')
    expect(horizontal).not.toHaveAttribute('placeholder')
    const vertical = skew.getByRole('textbox', { name: 'Vertical (°)' })
    expect(vertical).toHaveValue('')
    expect(vertical).toHaveAttribute('placeholder', 'Mixed')
  })

  it('shows 0 for boxes with no skew, a block with no translation left out', async () => {
    skewed({ one: { x: 12.5, y: 0 } })
    const { user } = renderBar([styled('one', {}), styled('two', {}), block('first', 'やあ')], {
      selected: ['two', 'first'],
    })

    const skew = await openSkew(user)

    expect(skew.getByRole('textbox', { name: 'Horizontal (°)' })).toHaveValue('0')
    expect(skew.getByRole('textbox', { name: 'Vertical (°)' })).toHaveValue('0')
  })

  it('leaves out a block styled before it was translated, which shows no skew either (P-d)', async () => {
    skewed({ one: { x: 12.5, y: 0 } })
    // Blank point text with the size it was given, as the type bar leaves it.
    const placeholder = { content: [], style: { chars: { size: 30 } }, frame: 'point' }
    const { user } = renderBar([
      styled('one', {}),
      block('first', 'やあ', { translation: placeholder }),
    ])

    const skew = await openSkew(user)

    expect(skew.getByRole('textbox', { name: 'Horizontal (°)' })).toHaveValue('12.5')
    expect(skew.getByRole('textbox', { name: 'Vertical (°)' })).toHaveValue('0')
  })

  it('shows angles that show alike as one, to the thousandth', async () => {
    skewed({ one: { x: 19.9999999, y: 0 }, two: { x: 20.0000001, y: -0.0000001 } })
    const { user } = renderBar([styled('one', {}), styled('two', {})])

    const skew = await openSkew(user)

    expect(skew.getByRole('textbox', { name: 'Horizontal (°)' })).toHaveValue('20')
    expect(skew.getByRole('textbox', { name: 'Vertical (°)' })).toHaveValue('0')
  })

  it('presses the fx button while a box shows a skew', () => {
    skewed({ two: { x: 0, y: 15 } })
    const { bar } = renderBar([styled('one', {}), styled('two', {})])

    expect(bar.getByRole('button', { name: 'Text effects' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
  })

  it('leaves the fx button up for boxes with no skew or gradient', () => {
    skewed({ other: { x: 10, y: 0 } })
    const { bar } = renderBar([styled('one', {})])

    expect(bar.getByRole('button', { name: 'Text effects' })).toHaveAttribute(
      'aria-pressed',
      'false',
    )
  })

  it('sends a typed Horizontal angle on Enter for that axis alone, once', async () => {
    skewed({ one: { x: 20, y: -5 }, two: { x: 0, y: 10 } })
    const { user } = renderBar([styled('one', {}), styled('two', {})])
    answers({ changed: 2 })

    const field = (await openSkew(user)).getByRole('textbox', { name: 'Horizontal (°)' })
    await user.type(field, '15{Enter}')
    fireEvent.blur(field)

    expect(commands.booharuSetSkew).toHaveBeenCalledExactlyOnceWith(['one', 'two'], 15, null)
    // The angle sent shows until the page's effects are read again.
    expect(field).toHaveValue('15')
  })

  it('sends a typed Vertical angle as the box loses the focus, for that axis alone', async () => {
    skewed({ one: { x: 20, y: -5 } })
    const { user } = renderBar([styled('one', {})])
    answers({ changed: 1 })

    const field = (await openSkew(user)).getByRole('textbox', { name: 'Vertical (°)' })
    await user.clear(field)
    await user.type(field, '-12')
    fireEvent.blur(field)

    expect(commands.booharuSetSkew).toHaveBeenCalledExactlyOnceWith(['one'], null, -12)
  })

  it('steps by 1 with the arrow keys, each step sent at once', async () => {
    skewed({ one: { x: 20, y: 0 } })
    const { user } = renderBar([styled('one', {})])
    answers({ changed: 1 })

    const field = (await openSkew(user)).getByRole('textbox', { name: 'Horizontal (°)' })
    fireEvent.keyDown(field, { key: 'ArrowUp' })

    expect(commands.booharuSetSkew).toHaveBeenCalledExactlyOnceWith(['one'], 21, null)
    expect(field).toHaveValue('21')
  })

  it('sends nothing for a cleared or unchanged box, which shows the skew again', async () => {
    skewed({ one: { x: 20, y: 0 } })
    const { user } = renderBar([styled('one', {})])

    const field = (await openSkew(user)).getByRole('textbox', { name: 'Horizontal (°)' })
    await user.clear(field)
    fireEvent.blur(field)
    expect(field).toHaveValue('20')

    await user.type(field, '{Enter}')
    expect(commands.booharuSetSkew).not.toHaveBeenCalled()
  })

  it('keeps from -60 to 60: a step past an end sends nothing, an angle beyond goes as that end', async () => {
    skewed({ one: { x: 60, y: -60 } })
    const { user } = renderBar([styled('one', {})])
    answers({ changed: 1 })

    const skew = await openSkew(user)
    const horizontal = skew.getByRole('textbox', { name: 'Horizontal (°)' })
    fireEvent.keyDown(horizontal, { key: 'ArrowUp' })
    const vertical = skew.getByRole('textbox', { name: 'Vertical (°)' })
    fireEvent.keyDown(vertical, { key: 'ArrowDown' })
    expect(horizontal).toHaveValue('60')
    expect(vertical).toHaveValue('-60')
    expect(commands.booharuSetSkew).not.toHaveBeenCalled()

    await user.clear(horizontal)
    await user.type(horizontal, '-75{Enter}')
    fireEvent.blur(horizontal)

    expect(commands.booharuSetSkew).toHaveBeenCalledExactlyOnceWith(['one'], -60, null)
  })

  it('takes the skew off along both axes with Reset skew', async () => {
    skewed({ one: { x: 20, y: -5 }, two: { x: 0, y: 10 } })
    const { user } = renderBar([styled('one', {}), styled('two', {})])
    answers({ changed: 2 })

    const skew = await openSkew(user)
    await user.click(skew.getByRole('button', { name: 'Reset skew' }))

    expect(commands.booharuSetSkew).toHaveBeenCalledExactlyOnceWith(['one', 'two'], 0, 0)
  })

  it('shows the skew again in place of a typed angle when Reset skew is pressed', async () => {
    skewed({ one: { x: 20, y: 0 } })
    const { user } = renderBar([styled('one', {})])
    answers({ changed: 1 })

    const skew = await openSkew(user)
    const field = skew.getByRole('textbox', { name: 'Horizontal (°)' })
    await user.clear(field)
    await user.type(field, '35')
    await user.click(skew.getByRole('button', { name: 'Reset skew' }))

    // Leaving the box for Reset sent the typed angle first, as the box commits as it loses focus.
    expect(commands.booharuSetSkew).toHaveBeenCalledTimes(2)
    expect(commands.booharuSetSkew).toHaveBeenNthCalledWith(1, ['one'], 35, null)
    expect(commands.booharuSetSkew).toHaveBeenNthCalledWith(2, ['one'], 0, 0)
    expect(screen.getByRole('textbox', { name: 'Horizontal (°)' })).toHaveValue('20')
    expect(field).not.toBeInTheDocument()
  })

  it('reports the locked and untranslated boxes it left, showing their skew again', async () => {
    skewed({ one: { x: 20, y: 0 }, two: { x: 20, y: 0 } })
    const { user } = renderBar([styled('one', {}), styled('two', {}), block('first', 'やあ')])
    answers({ locked: 2, untranslated: 1 })
    const shown = vi.spyOn(toast, 'add')

    const field = (await openSkew(user)).getByRole('textbox', { name: 'Horizontal (°)' })
    await user.clear(field)
    await user.type(field, '30{Enter}')

    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith({
        type: 'info',
        title: '2 locked boxes were left as they are. 1 box has no translation to skew.',
      }),
    )
    expect(commands.booharuSetSkew).toHaveBeenCalledExactlyOnceWith(
      ['one', 'two', 'first'],
      30,
      null,
    )
    // Nothing changed, so nothing reads the effects again: the box shows their skew at once,
    // keeping the focus, and its commit as it loses the focus sends nothing.
    await waitFor(() =>
      expect(screen.getByRole('textbox', { name: 'Horizontal (°)' })).toHaveValue('20'),
    )
    const fresh = screen.getByRole('textbox', { name: 'Horizontal (°)' })
    expect(fresh).toHaveFocus()
    fireEvent.blur(fresh)
    expect(commands.booharuSetSkew).toHaveBeenCalledOnce()
  })

  it('reports a locked box alone in its own words', async () => {
    skewed({})
    const { user } = renderBar([styled('one', {})])
    answers({ locked: 1 })
    const shown = vi.spyOn(toast, 'add')

    await user.click((await openSkew(user)).getByRole('button', { name: 'Reset skew' }))

    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith({
        type: 'info',
        title: '1 locked box was left as it is.',
      }),
    )
  })

  it('reports untranslated boxes alone in their own words', async () => {
    skewed({})
    const { user } = renderBar([block('first', 'やあ'), block('second', 'よう')])
    answers({ untranslated: 2 })
    const shown = vi.spyOn(toast, 'add')

    await user.click((await openSkew(user)).getByRole('button', { name: 'Reset skew' }))

    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith({
        type: 'info',
        title: '2 boxes have no translation to skew.',
      }),
    )
  })

  it('reports nothing when every box took the skew', async () => {
    skewed({})
    const { user } = renderBar([styled('one', {})])
    answers({ changed: 1 })
    const shown = vi.spyOn(toast, 'add')

    await user.click((await openSkew(user)).getByRole('button', { name: 'Reset skew' }))

    await waitFor(() => expect(commands.booharuSetSkew).toHaveBeenCalled())
    expect(shown).not.toHaveBeenCalled()
  })

  it('is off while the canvas types, saying to press Done first', async () => {
    skewed({ one: { x: 20, y: 0 } })
    const { user } = renderBar([text('one')], { typing: typing() })

    const skew = await openSkew(user)

    expect(skew.getByRole('textbox', { name: 'Horizontal (°)' })).toBeDisabled()
    expect(skew.getByRole('textbox', { name: 'Vertical (°)' })).toBeDisabled()
    expect(skew.getByRole('button', { name: 'Reset skew' })).toBeDisabled()
    await user.hover(screen.getByRole('group', { name: 'Skew' }))
    expect(
      await screen.findByText('Applies to the whole box: press Done first', {
        selector: '[data-slot=tooltip-content]',
      }),
    ).toBeInTheDocument()
  })

  it('is off for a new text being typed, which has no box yet', async () => {
    const { user } = renderBar([], { typing: typing(null), selected: [] })

    const skew = await openSkew(user)

    expect(skew.getByRole('textbox', { name: 'Horizontal (°)' })).toBeDisabled()
    expect(skew.getByRole('textbox', { name: 'Vertical (°)' })).toBeDisabled()
    expect(skew.getByRole('button', { name: 'Reset skew' })).toBeDisabled()
  })

  it('says nothing over the section while it is on, Reset skew naming itself', async () => {
    skewed({})
    const { user } = renderBar([styled('one', {})])

    const skew = await openSkew(user)
    await user.hover(skew.getByRole('button', { name: 'Reset skew' }))

    expect(
      await screen.findByText('Reset skew', { selector: '[data-slot=tooltip-content]' }),
    ).toBeInTheDocument()
    expect(screen.queryByText('Applies to the whole box: press Done first')).toBeNull()
  })
})
