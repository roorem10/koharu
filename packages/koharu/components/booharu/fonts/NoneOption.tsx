'use client'

import type { FontItem } from '@/lib/booharu/fonts'
import { ComboboxItem } from '@koharu/ui/components/combobox'

/** The leading row of Koharu's font list, `top` pixels down it, given a mount's `noneLabel`: the
 * list's item `item`, at `index` of its `count`, labelled with it, which stands for no family. The
 * arrows and Enter reach it as they reach a family, picking it gives an empty name, and it shows
 * as chosen while the value is empty. */
export function NoneOption({
  index,
  item,
  count,
  top,
}: {
  index: number
  item: FontItem
  count: number
  top: number
}) {
  return (
    <ComboboxItem
      index={index}
      value={item}
      aria-label={item.none} // Named as a family's row is.
      aria-setsize={count}
      aria-posinset={index + 1}
      // As a family's row looks, chosen or not, its label where a family's name starts.
      className='absolute inset-x-0 top-0 h-9.5 rounded-none pr-5.5 pl-5.5 data-[selected]:bg-accent data-[selected]:text-accent-foreground [&>span]:hidden!'
      style={{ transform: `translateY(${top}px)` }}
    >
      <div className='min-w-0 flex-1 truncate'>{item.none}</div>
    </ComboboxItem>
  )
}
