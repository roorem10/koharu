'use client'

import { useLayoutEffect, useState } from 'react'

/** The right pane's row of tabs as it's laid out, in CSS pixels. */
export interface TabRow {
  /** The width the row gives its tabs: its own, its padding left out. */
  available: number
  /** The gap between two tabs. */
  gap: number
  /** A tab's icon with the gap after it. */
  icon: number
  /** Each tab's label at its whole width, and what else it takes, icon aside: its padding and
   * borders, a badge, and the gaps between them. */
  tabs: readonly { label: number; fixed: number }[]
}

/** Whether every tab fits with its icon and its whole label. A sub-pixel short counts as fitting,
 * as widths come rounded. */
export function iconsFit({ available, gap, icon, tabs }: TabRow): boolean {
  const needed = tabs.reduce((sum, tab) => sum + icon + tab.label + tab.fixed, 0)
  return needed + gap * Math.max(tabs.length - 1, 0) <= available + 0.5
}

/** A tab's icon's size, Tailwind's `size-3`, which a hidden icon can't be measured at. */
const ICON = 12

const pixels = (value: string) => Number.parseFloat(value) || 0

/** Measures `list`, the right pane's row of tabs. A tab's label is its first part other than its
 * icon, whose whole width its scroll width gives while it's cut short; its other parts, such as
 * QC's count, never shrink. */
export function measureTabRow(list: HTMLElement): TabRow {
  const row = getComputedStyle(list)
  let gap = 0
  const tabs = Array.from(list.querySelectorAll<HTMLElement>('[role="tab"]'), (tab) => {
    const style = getComputedStyle(tab)
    gap = pixels(style.columnGap)
    const [label, ...others] = Array.from(tab.children).filter(
      (part): part is HTMLElement => part instanceof HTMLElement,
    )
    const box =
      pixels(style.paddingLeft) +
      pixels(style.paddingRight) +
      pixels(style.borderLeftWidth) +
      pixels(style.borderRightWidth)
    const fixed = others.reduce((sum, part) => sum + gap + part.getBoundingClientRect().width, box)
    return { label: label?.scrollWidth ?? 0, fixed }
  })
  return {
    available: list.clientWidth - pixels(row.paddingLeft) - pixels(row.paddingRight),
    gap: pixels(row.columnGap),
    icon: ICON + gap,
    tabs,
  }
}

/** Fits the right pane's five tabs to its width: each tab shows its icon while every label fits
 * whole beside them, and gives its icon's room to its label once one wouldn't, as at the pane's
 * 280 px minimum in a language with long names; a label still too long is cut short with an
 * ellipsis, its tab's title naming it whole. Spread over the row of tabs: its ref, measured again
 * as the row resizes or its tabs or labels change, and `data-booharu-icons`, `hidden` while the
 * icons give way, which the tabs' class hides them by. */
export function useSidebarTabsFit(): {
  ref: (list: HTMLElement | null) => void
  'data-booharu-icons': 'hidden' | undefined
} {
  const [list, setList] = useState<HTMLElement | null>(null)
  const [icons, setIcons] = useState(true)
  useLayoutEffect(() => {
    if (!list) return
    const fit = () => {
      // A row that isn't laid out, as in a collapsed pane, tells nothing of what fits.
      if (list.clientWidth > 0) setIcons(iconsFit(measureTabRow(list)))
    }
    fit()
    const resized = new ResizeObserver(fit)
    resized.observe(list)
    const changed = new MutationObserver(fit)
    changed.observe(list, { childList: true, characterData: true, subtree: true })
    let live = true
    void document.fonts?.ready.then(() => {
      if (live) fit()
    })
    return () => {
      live = false
      resized.disconnect()
      changed.disconnect()
    }
  }, [list])
  return { ref: setList, 'data-booharu-icons': icons ? undefined : 'hidden' }
}
