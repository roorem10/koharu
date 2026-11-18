'use client'

import { CirclePlay, type LucideIcon } from 'lucide-react'

import { RunsPreferences } from '@/components/booharu/runs/RunsPreferences'
import { booharuTabs, type BooharuTab } from '@/lib/booharu/settingsLink'

const icons: Record<BooharuTab, LucideIcon> = { runs: CirclePlay }

/** Booharu's Settings tabs with their icons, which SettingsPage lists after its own, so a later
 * Booharu tab is one more entry in `booharuTabs` and one more page below. */
export const booharuSettingsTabs = booharuTabs.map((tab) => [tab, icons[tab]] as const)

/** The page of Settings' `tab` when it's one of Booharu's tabs, and nothing otherwise. */
export function BooharuTabPage({ tab }: { tab: string }) {
  switch (tab) {
    case 'runs':
      return <RunsPreferences />
    default:
      return null
  }
}
