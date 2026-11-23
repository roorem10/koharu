'use client'

import { Children, isValidElement, type ComponentProps, type ReactNode } from 'react'

import { ContextBar as KoharuContextBar } from '@/components/editor/ContextBar'
import { cn } from '@koharu/ui/lib/utils'

/** Where a Booharu `ContextBar`'s next row starts. It draws nothing: the bar reads it. */
export function RowBreak() {
  return null
}

/** Koharu's ContextBar in rows, as the owner asked of the type bar: its children split at each
 * `RowBreak`, each row on a line of its own, lined up at the start under the row before, so the
 * bar is as wide as its widest row rather than as all of them. A row wraps in itself only when
 * the canvas is narrower than it, as Koharu's one row does. With no `RowBreak`, Koharu's bar as
 * it is. */
export function ContextBar({
  children,
  className,
  ...props
}: ComponentProps<typeof KoharuContextBar>) {
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
    <KoharuContextBar className={cn(className, 'flex-col flex-nowrap items-start')} {...props}>
      {rows
        .filter((cells) => cells.length > 0)
        .map((cells, index) => (
          <div
            // The rows never move, so their place is their key.
            key={index}
            data-booharu-row={index + 1}
            className='flex max-w-full flex-wrap items-center gap-1'
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
