import { act, render, screen, within } from '@testing-library/react'
import { StrictMode, type ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { ContextBar, RowBreak } from '@/components/booharu/typebar/Rows'
import type { Typed } from '@koharu/ipc/protocol'

import { renderBar, shownBar, text } from './typeBar'

/** The rows of the type bar, or of a Booharu ContextBar, as it shows them. */
function rowsOf(bar: HTMLElement): HTMLElement[] {
  return [...bar.querySelectorAll<HTMLElement>(':scope > [data-booharu-row]')]
}

/** The canvas typing into the text `text` with "Hello" selected. */
const typing: Typed = { layer: 'text', chars: {}, mixed: [], selected: 'Hello' }

/** jsdom's own styles, which `laidOut` starts from. */
const jsdomStyle = window.getComputedStyle.bind(window)

/** Lays out what's drawn as Chrome would for a Booharu ContextBar to measure, as jsdom lays out
 * nothing: each element as wide as its `data-w` says, and the bar, `data-testid='bar'`, with the
 * padding (6 px each side), border (1 px) and gap (4 px) Koharu's classes give it. */
function laidOut() {
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
    this: HTMLElement,
  ) {
    return { width: Number(this.dataset.w ?? 0) } as DOMRect
  })
  const bar: Record<string, string> = {
    paddingLeft: '6px',
    paddingRight: '6px',
    borderLeftWidth: '1px',
    borderRightWidth: '1px',
    columnGap: '4px',
  }
  vi.spyOn(window, 'getComputedStyle').mockImplementation((element, pseudo) => {
    const computed = jsdomStyle(element, pseudo)
    if (element.getAttribute('data-testid') !== 'bar') return computed
    return new Proxy(computed, {
      get(target, key) {
        if (typeof key === 'string' && key in bar) return bar[key]
        const value: unknown = Reflect.get(target, key, target)
        return typeof value === 'function' ? value.bind(target) : value
      },
    })
  })
}

/** The setup's resize observer, which tells nothing, put back after each test. */
const setupObserver = globalThis.ResizeObserver
afterEach(() => {
  globalThis.ResizeObserver = setupObserver
})

/** Resize observers that keep what they watch: a resize of `target`, told to those watching it,
 * how many watch it now, and how many were ever made to. */
function resizes() {
  class Observer {
    static made: Observer[] = []
    watched = new Set<Element>()
    ever = new Set<Element>()
    tell: ResizeObserverCallback
    constructor(tell: ResizeObserverCallback) {
      this.tell = tell
      Observer.made.push(this)
    }
    observe(target: Element) {
      this.watched.add(target)
      this.ever.add(target)
    }
    unobserve(target: Element) {
      this.watched.delete(target)
    }
    disconnect() {
      this.watched.clear()
    }
  }
  // The setup's can't be stubbed, as it's defined for good, but it can be written over.
  globalThis.ResizeObserver = Observer
  return {
    resize: (target: Element) =>
      act(() => {
        for (const observer of Observer.made) {
          if (!observer.watched.has(target)) continue
          observer.tell([{ target } as ResizeObserverEntry], observer)
        }
      }),
    watching: (target: Element) =>
      Observer.made.filter((observer) => observer.watched.has(target)).length,
    made: (target: Element) => Observer.made.filter((observer) => observer.ever.has(target)).length,
  }
}

/** A Booharu ContextBar on a canvas `canvas` px wide, as `laidOut` lays it out, which leaves its
 * controls 30 px less: A, B and C, 268 px with the gaps between them, then `more`, on the first
 * row, and D on the second, or on the one row with no RowBreak when `rows` is false. */
function Bar({
  canvas = 300,
  more,
  rows = true,
}: {
  canvas?: number
  more?: ReactNode
  rows?: boolean
}) {
  return (
    <div data-w={canvas} data-testid='canvas'>
      <ContextBar bounds={{ x: 0, y: 0, width: 10, height: 10 }} data-testid='bar'>
        <button type='button' data-w='100'>
          A
        </button>
        <button type='button' data-w='100'>
          B
        </button>
        <button type='button' data-w='60'>
          C
        </button>
        {more}
        {rows && <RowBreak />}
        <button type='button' data-w='100'>
          D
        </button>
      </ContextBar>
    </div>
  )
}

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

  it("is Koharu's one row, its controls kept, while a row is wider than the canvas leaves it", () => {
    laidOut()
    const { resize } = resizes()
    render(
      // A canvas 290 px wide leaves the bar's controls 260: 290 less Koharu's 8 px each side and
      // the bar's padding and border.
      <div data-w='290'>
        <ContextBar bounds={{ x: 0, y: 0, width: 10, height: 10 }} data-testid='bar'>
          <button type='button' data-w='100'>
            A
          </button>
          {/* A select's hidden input, fixed in place, takes no room in its row. */}
          <input
            aria-hidden='true'
            tabIndex={-1}
            readOnly
            style={{ position: 'fixed' }}
            data-w='1'
          />
          <button type='button' data-w='100'>
            B
          </button>
          <button type='button' data-w='60'>
            C
          </button>
          <RowBreak />
          <button type='button' data-w='100'>
            D
          </button>
        </ContextBar>
      </div>,
    )
    const bar = screen.getByTestId('bar')
    const canvas = bar.parentElement!
    const a = screen.getByText('A')
    // The first row needs 268, 100 + 4 + 100 + 4 + 60: one row, wrapping as Koharu's does, with
    // the rows laying out nothing.
    expect(bar).not.toHaveClass('flex-col')
    expect(bar).toHaveClass('items-center')
    expect(rowsOf(bar).map((row) => row.className)).toEqual(['contents', 'contents'])

    // 10 px wider, both rows fit the 270 left: the rows, the same controls in them.
    canvas.dataset.w = '300'
    resize(canvas)
    expect(bar).toHaveClass('flex-col', 'items-start')
    expect(rowsOf(bar).map((row) => row.textContent)).toEqual(['ABC', 'D'])
    expect(screen.getByText('A')).toBe(a)

    // A control grown too wide for its row is measured again too.
    const d = screen.getByText('D')
    d.dataset.w = '271'
    resize(d)
    expect(bar).not.toHaveClass('flex-col')
    expect(screen.getByText('A')).toBe(a)
  })

  it('measures again after a render that adds a control, with nothing resized', () => {
    laidOut()
    resizes()
    const { rerender } = render(<Bar />)
    // The 268 px the first row needs, of the 270 left.
    expect(screen.getByTestId('bar')).toHaveClass('flex-col')

    // As Applies to comes, with a render and no resize: 268 + 4 + 10 is too wide.
    rerender(<Bar more={<span data-w='10'>E</span>} />)
    expect(screen.getByTestId('bar')).not.toHaveClass('flex-col')
  })

  it('keeps the rows while a row is as wide as the room, no wider', () => {
    laidOut()
    resizes()
    // 298 less 30 leaves the 268 the first row needs.
    render(<Bar canvas={298} />)
    expect(screen.getByTestId('bar')).toHaveClass('flex-col')
  })

  it('takes no room for a control laid out on its own: hidden, or placed absolutely or fixed', () => {
    laidOut()
    resizes()
    render(
      <Bar
        more={
          <>
            <span data-w='50' style={{ display: 'none' }} />
            <span data-w='50' style={{ position: 'absolute' }} />
            <span data-w='50' style={{ position: 'fixed' }} />
          </>
        }
      />,
    )
    expect(screen.getByTestId('bar')).toHaveClass('flex-col')
  })

  it('leaves one observer watching the canvas, and none once the bar has no rows or is gone', () => {
    laidOut()
    const { watching } = resizes()
    // StrictMode mounts the bar, takes it down and mounts it again, as React may.
    const { rerender, unmount } = render(
      <StrictMode>
        <Bar />
      </StrictMode>,
    )
    const canvas = screen.getByTestId('canvas')
    expect(watching(canvas)).toBe(1)

    for (let n = 0; n < 25; n++) {
      rerender(
        <StrictMode>
          <Bar more={<span data-w='1'>{n}</span>} />
        </StrictMode>,
      )
    }
    expect(watching(canvas)).toBe(1)

    rerender(
      <StrictMode>
        <Bar rows={false} />
      </StrictMode>,
    )
    expect(watching(canvas)).toBe(0)
    // With a RowBreak again, it watches and measures again: the rows, as they fit.
    rerender(
      <StrictMode>
        <Bar />
      </StrictMode>,
    )
    expect(watching(canvas)).toBe(1)
    expect(screen.getByTestId('bar')).toHaveClass('flex-col')

    unmount()
    expect(watching(canvas)).toBe(0)
  })

  it('keeps one resize observer as long as it shows, watching the controls each render shows', () => {
    laidOut()
    const { made, resize, watching } = resizes()
    const { rerender } = render(<Bar />)
    const canvas = screen.getByTestId('canvas')
    for (let n = 0; n < 25; n++) rerender(<Bar more={<span data-w='1'>{n}</span>} />)
    // However often the bar renders, so a render while the browser tells of a resize never makes
    // another, whose first notices the browser would put off to the next frame.
    expect(made(canvas)).toBe(1)

    // What it watches follows the controls: E while it shows, and no longer once it's gone.
    rerender(<Bar more={<span data-w='10'>E</span>} />)
    const e = screen.getByText('E')
    expect(watching(e)).toBe(1)
    rerender(<Bar />)
    expect(watching(e)).toBe(0)

    // And it still tells the bar of a resize: 290 leaves 260, too little for the first row.
    expect(screen.getByTestId('bar')).toHaveClass('flex-col')
    canvas.dataset.w = '290'
    resize(canvas)
    expect(screen.getByTestId('bar')).not.toHaveClass('flex-col')
  })
})
