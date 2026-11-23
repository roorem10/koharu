import { render, screen, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { ContextBar, RowBreak } from '@/components/booharu/typebar/Rows'
import type { Typed } from '@koharu/ipc/protocol'

import { renderBar, shownBar, text } from './typeBar'

/** The rows of the type bar, or of a Booharu ContextBar, as it shows them. */
function rowsOf(bar: HTMLElement): HTMLElement[] {
  return [...bar.querySelectorAll<HTMLElement>(':scope > [data-booharu-row]')]
}

/** The canvas typing into the text `text` with "Hello" selected. */
const typing: Typed = { layer: 'text', chars: {}, mixed: [], selected: 'Hello' }

describe('Booharu type bar in two rows (the owner asked, 2026-11-23)', () => {
  it('puts the font to the size on the first row and the rest on the second', async () => {
    await renderBar([text('text')])

    const [first, second, ...more] = rowsOf(await shownBar())
    expect(more).toEqual([])
    const top = within(first!)
    expect(top.getByRole('combobox', { name: 'Font weight' })).toBeInTheDocument()
    for (const name of ['Bold', 'Italic', 'Emphasis']) {
      expect(top.getByRole('button', { name })).toBeInTheDocument()
    }
    expect(top.getByTestId('type-size')).toBeInTheDocument()
    expect(top.queryByRole('button', { name: 'Text color' })).toBeNull()

    const bottom = within(second!)
    for (const name of [
      'Text color',
      'Align center',
      'Spacing',
      'Vertical',
      'Border color',
      'More border options',
      'Text effects',
    ]) {
      expect(bottom.getByRole('button', { name })).toBeInTheDocument()
    }
    expect(bottom.getByRole('textbox', { name: 'Border width' })).toBeInTheDocument()
    expect(bottom.queryByTestId('type-size')).toBeNull()
  })

  it('opens the first row with Applies to and the second with A− and A+, Done last, while typing', async () => {
    await renderBar([text('text')], { typing })

    const [first, second] = rowsOf(await shownBar())
    expect(first!.firstElementChild).toHaveTextContent(/^Applies to "Hello"Whole box$/)
    const steps = within(second!).getByRole('button', { name: 'Smaller' })
    expect(second!.firstElementChild).toContainElement(steps)
    expect(within(second!).getByRole('button', { name: 'Bigger' })).toBeInTheDocument()
    expect(second!.lastElementChild).toHaveTextContent('Done')
  })

  it('leaves a bar with no RowBreak as Koharu draws it, and draws no empty row', () => {
    const bounds = { x: 0, y: 0, width: 10, height: 10 }
    const { rerender } = render(
      <ContextBar bounds={bounds} data-testid='bar'>
        <button type='button'>A</button>
        <button type='button'>B</button>
      </ContextBar>,
    )
    const bar = screen.getByTestId('bar')
    expect(rowsOf(bar)).toEqual([])
    expect([...bar.children].map((child) => child.textContent)).toEqual(['A', 'B'])
    expect(bar).toHaveClass('items-center')

    rerender(
      <ContextBar bounds={bounds} data-testid='bar'>
        <RowBreak />
        <button type='button'>A</button>
        <RowBreak />
        <RowBreak />
        <button type='button'>B</button>
        <RowBreak />
      </ContextBar>,
    )
    const rows = rowsOf(screen.getByTestId('bar'))
    expect(rows.map((row) => row.textContent)).toEqual(['A', 'B'])
    expect(rows.map((row) => row.dataset.booharuRow)).toEqual(['1', '2'])
    expect(screen.getByTestId('bar')).toHaveClass('flex-col', 'items-start')
  })
})
