'use client'

import type { VirtualItem } from '@tanstack/react-virtual'
import { useTranslation } from 'react-i18next'

/** The headings of Koharu's font list's sections over its rows in view, `rows`, each over the row
 * `headings` names it for, as ModelPicker heads each vendor's models. The row's height takes the
 * heading's in. */
export function FontHeadings({
  rows,
  headings,
}: {
  rows: readonly VirtualItem[]
  headings: ReadonlyMap<number, 'favorites' | 'allFonts'>
}) {
  return (
    <>
      {rows.map((row) => {
        const heading = headings.get(row.index)
        return heading && <FontHeading key={heading} heading={heading} top={row.start} />
      })}
    </>
  )
}

/** A section's heading in Koharu's font list, `top` pixels down it. */
export function FontHeading({ heading, top }: { heading: 'favorites' | 'allFonts'; top: number }) {
  const { t } = useTranslation()
  return (
    <div
      role='presentation'
      className='absolute inset-x-0 top-0 flex h-6 items-end truncate px-2 pb-0.5 text-[10px] font-medium text-muted-foreground'
      style={{ transform: `translateY(${top}px)` }}
    >
      {t(`booharu.fonts.${heading}`)}
    </div>
  )
}
