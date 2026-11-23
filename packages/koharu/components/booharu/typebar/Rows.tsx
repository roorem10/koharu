'use client'

import {
  Children,
  isValidElement,
  useLayoutEffect,
  useRef,
  useState,
  type ComponentProps,
  type ReactNode,
} from 'react'

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
  // After each render, as the controls change, and as the canvas or a control is resized.
  useLayoutEffect(() => {
    const bar = firstRow.current?.parentElement
    const canvas = bar?.parentElement
    if (!bar || !canvas) return
    const measure = () => {
      // A canvas that isn't laid out, as in jsdom, tells nothing of what fits.
      const width = canvas.getBoundingClientRect().width
      if (!width) return
      const style = getComputedStyle(bar)
      const px = (value: string) => parseFloat(value) || 0
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
      setFlowing(rowsOf(bar).some((row) => widthOf(row) > room))
    }
    measure()
    const resized = new ResizeObserver(measure)
    resized.observe(canvas)
    for (const row of rowsOf(bar)) for (const cell of cellsOf(row)) resized.observe(cell)
    return () => resized.disconnect()
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
