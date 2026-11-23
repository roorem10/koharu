'use client'

import {
  Children,
  isValidElement,
  startTransition,
  useLayoutEffect,
  useRef,
  useState,
  type ComponentProps,
  type ReactNode,
} from 'react'
import { flushSync } from 'react-dom'

import { ContextBar as KoharuContextBar } from '@/components/editor/ContextBar'
import { cn } from '@koharu/ui/lib/utils'

/** The canvas's width Koharu's bar keeps clear, 8 px at each side: its `max-w-[calc(100%-16px)]`. */
const MARGIN = 16

/** What gives way in a row, cut to nothing before the bar goes to one row: a part of a control, as
 * Applies to's quote, or a control itself. The row lets the control that is or holds it narrow, and
 * the measure counts it as nothing, so it goes on a box cut to fit (`min-w-0 truncate`), each box
 * from it up to the control letting it narrow too (`min-w-0`) and the control's other parts
 * keeping their widths (`shrink-0`), as Applies to's do. */
const GIVES_WAY = '[data-booharu-gives-way]'

/** Where a Booharu `ContextBar`'s next row starts. It draws nothing: the bar reads it. */
export function RowBreak() {
  return null
}

/** Koharu's ContextBar in rows, as the owner asked of the type bar: its children split at each
 * `RowBreak`, each row on a line of its own, lined up at the start under the row before, so the
 * bar is as wide as its widest row rather than as all of them. A row never wraps: what's marked
 * `data-booharu-gives-way` (`GIVES_WAY`), as Applies to's quote, narrows to fit it, and the row's
 * other controls keep their widths. While a row is wider than the room Koharu's bar has on the
 * canvas even with that cut to nothing, as the second is while typing in a window narrower
 * than about 950 px and the first with characters chosen in one narrower than about 955, the bar
 * is Koharu's one row instead, wrapping at the canvas's edge; its rows then lay nothing out
 * (`contents`), so no control remounts as the bar goes from one to the other. With no `RowBreak`,
 * Koharu's bar as it is. */
export function ContextBar({
  children,
  className,
  ...props
}: ComponentProps<typeof KoharuContextBar>) {
  const firstRow = useRef<HTMLDivElement>(null)
  const [flowing, setFlowing] = useState(false)
  // The measure of the rows the latest render left, which the observer below runs too, with what
  // it tells of.
  const measure = useRef<(resized?: readonly ResizeObserverEntry[]) => void>(null)
  // One resize observer for as long as the bar shows, and what it watches: a render never makes
  // another, whose first notices, made while the browser tells the others of a resize, it would
  // put off to the next frame, warning of a loop.
  const observer = useRef<{ resized: ResizeObserver; watched: Set<Element> }>(null)
  useLayoutEffect(() => {
    const own = {
      resized: new ResizeObserver((entries) => measure.current?.(entries)),
      watched: new Set<Element>(),
    }
    observer.current = own
    return () => {
      own.resized.disconnect()
      observer.current = null
    }
  }, [])
  // After each render, as the controls change: the canvas and the controls shown watched, and the
  // rows measured.
  useLayoutEffect(() => {
    const bar = firstRow.current?.parentElement
    const canvas = bar?.parentElement
    const own = observer.current
    if (own) {
      const watching = new Set(bar && canvas ? [canvas, ...rowsOf(bar).flatMap(cellsOf)] : [])
      for (const target of own.watched) if (!watching.has(target)) own.resized.unobserve(target)
      for (const target of watching) if (!own.watched.has(target)) own.resized.observe(target)
      own.watched = watching
    }
    measure.current =
      bar && canvas
        ? (resized) => {
            const flows = overflows(bar, canvas)
            if (flows === undefined) return
            // After a render, and as the canvas is resized, the bar goes from one to the other
            // before the browser paints, never showing a row too wide for it: the canvas being the
            // shallowest thing watched, the browser then tells floating-ui of the bar's new size
            // in the same frame. As a control is resized alone, after the paint, in a transition,
            // which floating-ui's flushSync as it places the bar leaves for later: going in this
            // frame would change the bar's size after floating-ui is told of it, which the browser
            // leaves to the next frame, warning of a loop.
            if (!resized) setFlowing(flows)
            else if (resized.some(({ target }) => target === canvas))
              flushSync(() => setFlowing(flows))
            else startTransition(() => setFlowing(flows))
          }
        : null
    measure.current?.()
  })

  const items = Children.toArray(children)
  if (!items.some(isRowBreak)) {
    return (
      <KoharuContextBar className={className} {...props}>
        {children}
      </KoharuContextBar>
    )
  }

  const rows: ReactNode[][] = []
  let row: ReactNode[] = []
  for (const item of items) {
    if (!isRowBreak(item)) {
      row.push(item)
      continue
    }
    rows.push(row)
    row = []
  }
  rows.push(row)

  return (
    <KoharuContextBar
      className={flowing ? className : cn(className, 'flex-col flex-nowrap items-start')}
      {...props}
    >
      {rows
        .filter((cells) => cells.length > 0)
        .map((cells, index) => (
          <div
            // The rows never move, so their place is their key.
            key={index}
            ref={index === 0 ? firstRow : undefined}
            data-booharu-row={index + 1}
            // Laid out, a row keeps each control's width but that of one that gives way or holds a
            // part that does (`GIVES_WAY`, spelled out for Tailwind to read).
            className={
              flowing
                ? 'contents'
                : 'flex max-w-full items-center gap-1 [&>:not([data-booharu-gives-way]):not(:has([data-booharu-gives-way]))]:shrink-0'
            }
          >
            {cells}
          </div>
        ))}
    </KoharuContextBar>
  )
}

function isRowBreak(node: ReactNode): boolean {
  return isValidElement(node) && node.type === RowBreak
}

/** Whether a row of `bar`'s is wider than the room Koharu's bar has on `canvas`, its parent, at its
 * narrowest; nothing where the canvas isn't laid out, as in jsdom, which tells nothing of what
 * fits. */
function overflows(bar: Element, canvas: Element): boolean | undefined {
  const width = canvas.getBoundingClientRect().width
  if (!width) return undefined
  const style = getComputedStyle(bar)
  // The room Koharu's bar has: the canvas's width but for its margin, padding and border.
  const room =
    width -
    MARGIN -
    px(style.paddingLeft) -
    px(style.paddingRight) -
    px(style.borderLeftWidth) -
    px(style.borderRightWidth)
  // A row's width is its controls' at their narrowest and the gaps between them, the rows' `gap-1`
  // being the bar's own, so it's the same however the bar lays them out, and the bar never flips
  // back and forth.
  const gap = px(style.columnGap)
  return rowsOf(bar).some((row) => lined(cellsOf(row), gap) > room)
}

/** How wide `cells` are at their narrowest in a line, `gap` apart. */
function lined(cells: Element[], gap: number): number {
  return cells.reduce((sum, cell) => sum + gap + leastWidth(cell), -gap)
}

/** How narrow `cell` can be: nothing where it gives way itself; as wide as it is; or, holding a part
 * that gives way, as its parts in a line with that part cut to nothing, however wide it is now. */
function leastWidth(cell: Element): number {
  if (cell.matches(GIVES_WAY)) return 0
  if (!cell.querySelector(GIVES_WAY)) return cell.getBoundingClientRect().width
  const style = getComputedStyle(cell)
  return (
    lined(cellsOf(cell), px(style.columnGap)) +
    px(style.paddingLeft) +
    px(style.paddingRight) +
    px(style.borderLeftWidth) +
    px(style.borderRightWidth)
  )
}

/** A length in CSS pixels, 0 for none. */
function px(value: string): number {
  return parseFloat(value) || 0
}

/** The rows `bar` shows. */
function rowsOf(bar: Element): Element[] {
  return [...bar.querySelectorAll(':scope > [data-booharu-row]')]
}

/** The controls `row` lines up, or the parts a control does: its children, but for any laid out on
 * their own, as a select's hidden input, fixed in place, is. */
function cellsOf(row: Element): Element[] {
  return [...row.children].filter((cell) => {
    const { display, position } = getComputedStyle(cell)
    return display !== 'none' && position !== 'absolute' && position !== 'fixed'
  })
}
