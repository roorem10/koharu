'use client'

import { useLayoutEffect, useState } from 'react'

/** The right pane's row of tabs as it's laid out, in CSS pixels. */
export interface TabRow {
  /** The width the row gives its tabs: its own, its padding and borders left out. */
  available: number
  /** The gap between two tabs. */
  gap: number
  /** Each tab's icon with the gap after it, 0 without one; its label at its whole width; and what
   * else it takes, none of which shrinks: its padding and borders, a badge, and the gaps between
   * them. */
  tabs: readonly { icon: number; label: number; fixed: number }[]
}

/** Whether every tab fits with its icon and its whole label. Widths come in layout units, a 64th
 * of a pixel, so only a hundredth over, a sum's rounding, still fits. */
export function iconsFit({ available, gap, tabs }: TabRow): boolean {
  const needed = tabs.reduce((sum, tab) => sum + tab.icon + tab.label + tab.fixed, 0)
  return needed + gap * Math.max(tabs.length - 1, 0) <= available + 0.01
}

const pixels = (value: string) => Number.parseFloat(value) || 0

/** What `style` gives an element's padding and borders across. */
const across = (style: CSSStyleDeclaration) =>
  pixels(style.paddingLeft) +
  pixels(style.paddingRight) +
  pixels(style.borderLeftWidth) +
  pixels(style.borderRightWidth)

/** Measures `list`, the right pane's row of tabs. A tab's label is its first part other than its
 * icon; its other parts, such as QC's count, never shrink. The tabs are held from shrinking while
 * they're measured, so that each label takes its whole width, and shrink again before anything
 * paints. An icon is measured by its style, which gives its size while it's hidden too. */
export function measureTabRow(list: HTMLElement): TabRow {
  const triggers = Array.from(list.querySelectorAll<HTMLElement>('[role="tab"]'))
  const shrinks = triggers.map((tab) => tab.style.flexShrink)
  for (const tab of triggers) tab.style.flexShrink = '0'
  try {
    const tabs = triggers.map((tab) => {
      const style = getComputedStyle(tab)
      const gap = pixels(style.columnGap)
      const parts = Array.from(tab.children)
      const icon = parts.find((part) => part instanceof SVGElement)
      const [label, ...others] = parts.filter((part) => part instanceof HTMLElement)
      return {
        icon: icon
          ? (pixels(getComputedStyle(icon).width) || icon.getBoundingClientRect().width) + gap
          : 0,
        label: label?.getBoundingClientRect().width ?? 0,
        fixed: others.reduce(
          (sum, part) => sum + gap + part.getBoundingClientRect().width,
          across(style),
        ),
      }
    })
    const row = getComputedStyle(list)
    return {
      available: list.getBoundingClientRect().width - across(row),
      gap: pixels(row.columnGap),
      tabs,
    }
  } finally {
    triggers.forEach((tab, index) => (tab.style.flexShrink = shrinks[index] ?? ''))
  }
}

/** Fits the right pane's tabs to its width: each tab shows its icon while every label fits whole
 * beside them, and gives its icon's room to its label once one wouldn't, as in English at the
 * pane's default 340 px once the QC tab shows; a label still too long, as at its 280 px minimum in
 * a language with long names, is cut short with an ellipsis, its tab's title naming it whole.
 * Spread over the row of tabs: its ref, measured again as the row resizes or its tabs or labels
 * change, and `data-booharu-icons`, `hidden` while the icons give way, which the tabs' class hides
 * them by. */
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
    // The tabs and their text, not their attributes, which the measure's own style changes are.
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
