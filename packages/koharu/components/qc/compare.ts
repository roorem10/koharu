'use client'

import { create } from 'zustand'

// Comparison is transient presentation state and never crosses a project edit command.
export const useCompareStore = create<{
  pinned: boolean
  held: boolean
  toggle: () => void
  setHeld: (held: boolean) => void
  reset: () => void
}>()((set) => ({
  pinned: false,
  held: false,
  toggle: () => set((state) => ({ pinned: !state.pinned })),
  setHeld: (held) => set({ held }),
  reset: () => set({ pinned: false, held: false }),
}))

export const originalVisible = () => {
  const state = useCompareStore.getState()
  return state.pinned || state.held
}
