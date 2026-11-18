'use client'

import { useEffect } from 'react'

import { useKoharuStore } from '@/lib/store'

/** The tabs Booharu adds to the Settings page, after Koharu's, each with its name at
 * `settings.tabs.<tab>` in booharu.json, where Koharu's tab list looks a tab's name up. */
export const booharuTabs = ['runs'] as const
export type BooharuTab = (typeof booharuTabs)[number]

/** Whether `tab` is one of Booharu's Settings tabs, whose page reads only Booharu's settings. */
export function isBooharuTab(tab: string): tab is BooharuTab {
  return (booharuTabs as readonly string[]).includes(tab)
}

/** The Settings page's tabs, by SettingsPage's names for them. */
export type SettingsTab = 'appearance' | 'pipeline' | 'providers' | 'typesetting' | BooharuTab

/** How long a link waits for the section it goes to to show, in milliseconds. */
const revealWait = 2000

/** The tab setter of the Settings page, while it's mounted. */
let showTab: ((tab: SettingsTab) => void) | null = null

/** A link followed while Settings was closed, which the page follows once it's mounted. */
let waiting: { tab: SettingsTab; section: string } | null = null

/** Lets a Booharu link open a Settings tab: SettingsPage hands its tab setter over with this one
 * hook call, for as long as it's mounted, and follows a link that opened it. */
export function useSettingsLinks(show: (tab: SettingsTab) => void): void {
  useEffect(() => {
    showTab = show
    const link = waiting
    waiting = null
    if (link) reveal(link.tab, link.section)
    return () => {
      if (showTab === show) showTab = null
    }
  }, [show])
}

/** Shows Settings' `tab`, then scrolls the element whose id is `section` into view and moves focus
 * to it once it shows, as the link that opened it went with the tab it was on. From outside
 * Settings, such as from the editor, Settings opens first. */
export function openSettingsSection(tab: SettingsTab, section: string): void {
  if (showTab) {
    reveal(tab, section)
    return
  }
  waiting = { tab, section }
  useKoharuStore.getState().setSettingsOpen(true)
}

function reveal(tab: SettingsTab, section: string): void {
  showTab?.(tab)
  focusSection(section, performance.now() + revealWait)
}

/** Scrolls to the element whose id is `section`, and focuses it, a frame from now, or each frame
 * after until it shows, as a tab's page can wait for its settings, or until `until`, a
 * `performance.now()` time. */
function focusSection(section: string, until: number): void {
  requestAnimationFrame(() => {
    const target = document.getElementById(section)
    if (!target) {
      if (performance.now() < until) focusSection(section, until)
      return
    }
    target.scrollIntoView({ block: 'start' })
    target.focus({ preventScroll: true })
  })
}
