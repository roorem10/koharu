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

/** Where a Booharu `ContextBar`'s next row starts. It draws nothing: the bar reads it. */
export function RowBreak() {
  return null
}

/** Koharu's ContextBar in rows, as the owner asked of the type bar: its children split at each
 * `RowBreak`, each row on a line of its own, lined up at the start under the row before, so the
 * bar is as wide as its widest row rather than as all of them. While a row is wider than the room
 * Koharu's bar has on the canvas, as the first is while typing in Booharu's starting window, the
 * bar is Koharu's one row instead, wrapping at the canvas's edge, which never takes more lines
 * than the rows would, each wrapping in itself; its rows then lay nothing out (`contents`), so no
 * control remounts as the bar goes from one to the other. With no `RowBreak`, Koharu's bar as it
 * is. */
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
            // before the browser paints, never showing a row wrapped in itself: the canvas being
            // the shallowest thing watched, the browser then tells floating-ui of the bar's new
            // size in the same frame. As a control is resized alone, after the paint, in a
            // transition, which floating-ui's flushSync as it places the bar leaves for later:
            // going in this frame would change the bar's size after floating-ui is told of it,
            // which the browser leaves to the next frame, warning of a loop.
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
            className={flowing ? 'contents' : 'flex max-w-full flex-wrap items-center gap-1'}
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

/** Whether a row of `bar`'s is wider than the room Koharu's bar has on `canvas`, its parent; nothing
 * where the canvas isn't laid out, as in jsdom, which tells nothing of what fits. */
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
  // A row's width is its controls' and the gaps between them, the rows' `gap-1` being the bar's
  // own, so it's the same however the bar lays them out, and the bar never flips back and forth.
  const gap = px(style.columnGap)
  const widthOf = (row: Element) =>
    cellsOf(row).reduce((sum, cell) => sum + gap + cell.getBoundingClientRect().width, -gap)
  return rowsOf(bar).some((row) => widthOf(row) > room)
}

/** A length in CSS pixels, 0 for none. */
function px(value: string): number {
  return parseFloat(value) || 0
}

/** The rows `bar` shows. */
function rowsOf(bar: Element): Element[] {
  return [...bar.querySelectorAll(':scope > [data-booharu-row]')]
}

/** The controls `row` lines up: its children, but for any laid out on their own, as a select's
 * hidden input, fixed in place, is. */
function cellsOf(row: Element): Element[] {
  return [...row.children].filter((cell) => {
    const { display, position } = getComputedStyle(cell)
    return display !== 'none' && position !== 'absolute' && position !== 'fixed'
  })
}
