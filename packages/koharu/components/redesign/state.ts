'use client'

import { create } from 'zustand'

import type { EntityId } from '@koharu/bridge/protocol'

export type TypeTab = 'text' | 'outline' | 'fill'
type Popup = { kind: 'fx'; targets: EntityId[] } | { kind: 'details'; layer: EntityId } | null

// Session UI state is independent of saved project state and the canvas tool store.
export const useRedesignStore = create<{
  tab: TypeTab
  popup: Popup
  setTab: (tab: TypeTab) => void
  openFx: (targets: EntityId[]) => void
  openDetails: (layer: EntityId) => void
  close: () => void
}>()((set) => ({
  tab: 'text',
  popup: null,
  setTab: (tab) => set({ tab }),
  openFx: (targets) => set({ popup: { kind: 'fx', targets: [...new Set(targets)] } }),
  openDetails: (layer) => set({ popup: { kind: 'details', layer } }),
  close: () => set({ popup: null }),
}))
