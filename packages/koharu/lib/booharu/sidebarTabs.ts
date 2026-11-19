'use client'

import { useLayoutEffect, useState } from 'react'

/** The right pane's row of tabs as it's laid out, in CSS pixels. */
export interface TabRow {
  /** The width the row gives its tabs: its own, its padding and borders left out. */
  available: number
  /** The gap between two tabs, and that while they're tight. */
  gap: number
  tightGap: number
  /** Each tab's icon with the gap after it, 0 without one; its label at its whole width; what else
   * it takes, none of which shrinks: its padding and borders, a badge, and the gaps between them;
   * and that while the tabs are tight. */
  tabs: readonly { icon: number; label: number; fixed: number; tightFixed: number }[]
}

/** How the right pane's tabs fit its width, the first of these that does: `whole`, each tab with its
 * icon and its whole label; `labels`, the whole labels without the icons; `tight`, the whole labels
 * with less room around and between the tabs; and `icons`, the icons alone, each tab named in its
 * title and to screen readers, never a label cut to a letter or two. */
export type TabsFit = 'whole' | 'labels' | 'tight' | 'icons'

/** How `row`'s tabs fit. Widths come in layout units, a 64th of a pixel, so only a hundredth over,
 * a sum's rounding, still fits. */
export function tabsFit({ available, gap, tightGap, tabs }: TabRow): TabsFit {
  const fits = (width: (tab: TabRow['tabs'][number]) => number, between: number) =>
    tabs.reduce((sum, tab) => sum + width(tab), 0) + between * Math.max(tabs.length - 1, 0) <=
    available + 0.01
  if (fits((tab) => tab.icon + tab.label + tab.fixed, gap)) return 'whole'
  if (fits((tab) => tab.label + tab.fixed, gap)) return 'labels'
  if (fits((tab) => tab.label + tab.tightFixed, tightGap)) return 'tight'
  return 'icons'
}

/** The attribute the row of tabs says how they fit by, which their classes follow; none while they
 * fit whole. */
const FIT = 'data-booharu-tabs'

const pixels = (value: string) => Number.parseFloat(value) || 0

/** What `style` gives an element's padding and borders across. */
const across = (style: CSSStyleDeclaration) =>
  pixels(style.paddingLeft) +
  pixels(style.paddingRight) +
  pixels(style.borderLeftWidth) +
  pixels(style.borderRightWidth)

/** `tab`'s icon, its label, its first part other than the icon, and its other parts, such as QC's
 * count, which never shrink. */
function parts(tab: HTMLElement) {
  const children = Array.from(tab.children)
  const icon = children.find((part) => part instanceof SVGElement)
  const [label, ...others] = children.filter((part) => part instanceof HTMLElement)
  return { icon, label, others }
}

/** What `tab` takes as its style is now besides its icon and its label: its padding and borders,
 * and its other parts with the gap before each. */
function fixedOf(tab: HTMLElement): number {
  const style = getComputedStyle(tab)
  const gap = pixels(style.columnGap)
  return parts(tab).others.reduce(
    (sum, part) => sum + gap + part.getBoundingClientRect().width,
    across(style),
  )
}

/** Measures `list`, the right pane's row of tabs, as they'd be whole and as they'd be tight, however
 * they fit now: they're held from shrinking meanwhile, so that each label takes its whole width,
 * and go back to how they fit before anything paints. An icon is measured by its style, which gives
 * its size while it's hidden too. */
export function measureTabRow(list: HTMLElement): TabRow {
  const fit = list.getAttribute(FIT)
  const triggers = Array.from(list.querySelectorAll<HTMLElement>('[role="tab"]'))
  const shrinks = triggers.map((tab) => tab.style.flexShrink)
  for (const tab of triggers) tab.style.flexShrink = '0'
  try {
    list.removeAttribute(FIT)
    const tabs = triggers.map((tab) => {
      const { icon, label } = parts(tab)
      return {
        icon: icon
          ? (pixels(getComputedStyle(icon).width) || icon.getBoundingClientRect().width) +
            pixels(getComputedStyle(tab).columnGap)
          : 0,
        label: label?.getBoundingClientRect().width ?? 0,
        fixed: fixedOf(tab),
      }
    })
    const row = getComputedStyle(list)
    const available = list.getBoundingClientRect().width - across(row)
    const gap = pixels(row.columnGap)
    list.setAttribute(FIT, 'tight')
    const tightGap = pixels(getComputedStyle(list).columnGap)
    return {
      available,
      gap,
      tightGap,
      tabs: tabs.map((tab, index) => ({ ...tab, tightFixed: fixedOf(triggers[index]!) })),
    }
  } finally {
    if (fit === null) list.removeAttribute(FIT)
    else list.setAttribute(FIT, fit)
    triggers.forEach((tab, index) => (tab.style.flexShrink = shrinks[index] ?? ''))
  }
}

/** Fits the right pane's tabs to its width, as `tabsFit` tells: each tab shows its icon while every
 * label fits whole beside them; gives its icon's room to its label once one wouldn't, as in English
 * at the pane's default 340 px once the QC tab shows; takes less room around itself once the labels
 * still don't fit, as in Japanese there, or in Portuguese at its 280 px minimum; and shows its icon
 * alone once even that's too narrow, as in Japanese at 280 px, its title and screen readers naming
 * it. Spread over the row of tabs: its ref, measured again as the row resizes or its tabs or labels
 * change, and `data-booharu-tabs`, how they fit while it's other than whole, which the tabs' class
 * follows. */
export function useSidebarTabsFit(): {
  ref: (list: HTMLElement | null) => void
  'data-booharu-tabs': Exclude<TabsFit, 'whole'> | undefined
} {
  const [list, setList] = useState<HTMLElement | null>(null)
  const [fit, setFit] = useState<TabsFit>('whole')
  useLayoutEffect(() => {
    if (!list) return
    const measure = () => {
      // A row that isn't laid out, as in a collapsed pane, tells nothing of what fits.
      if (list.clientWidth > 0) setFit(tabsFit(measureTabRow(list)))
    }
    measure()
    const resized = new ResizeObserver(measure)
    resized.observe(list)
    // The tabs and their text, not their attributes, which the measure's own changes are.
    const changed = new MutationObserver(measure)
    changed.observe(list, { childList: true, characterData: true, subtree: true })
    let live = true
    void document.fonts?.ready.then(() => {
      if (live) measure()
    })
    return () => {
      live = false
      resized.disconnect()
      changed.disconnect()
    }
  }, [list])
  return { ref: setList, 'data-booharu-tabs': fit === 'whole' ? undefined : fit }
}
