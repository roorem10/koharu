'use client'

import { useEffect } from 'react'

/** The tabs Booharu adds to the Settings page, after Koharu's, each with its name at
 * `settings.tabs.<tab>` in booharu.json, where Koharu's tab list looks a tab's name up. */
export const booharuTabs = ['runs'] as const
export type BooharuTab = (typeof booharuTabs)[number]

/** The Settings page's tabs, by SettingsPage's names for them. */
export type SettingsTab = 'appearance' | 'pipeline' | 'providers' | 'typesetting' | BooharuTab

/** The tab setter of the Settings page, while it's mounted. */
let showTab: ((tab: SettingsTab) => void) | null = null

/** Lets a Booharu link on one Settings tab open another: SettingsPage hands its tab setter over
 * with this one hook call, for as long as it's mounted. */
export function useSettingsLinks(show: (tab: SettingsTab) => void): void {
  useEffect(() => {
    showTab = show
    return () => {
      if (showTab === show) showTab = null
    }
  }, [show])
}

/** Shows Settings' `tab`, then scrolls the element whose id is `section` into view and moves focus
 * to it, as the link that opened it went with the tab it was on. */
export function openSettingsSection(tab: SettingsTab, section: string): void {
  showTab?.(tab)
  requestAnimationFrame(() => {
    const target = document.getElementById(section)
    target?.scrollIntoView({ block: 'start' })
    target?.focus({ preventScroll: true })
  })
}
