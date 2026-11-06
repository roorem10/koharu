'use client'

import { create } from 'zustand'
import { persist } from 'zustand/middleware'

export interface CanvasGuide {
  id: string
  axis: 'x' | 'y'
  position: number
}

interface CanvasViewPreferences {
  rulers: boolean
  grid: boolean
  gridSize: number
  snapping: boolean
}

interface CanvasViewStore extends CanvasViewPreferences {
  guides: CanvasGuide[]
  setRulers: (rulers: boolean) => void
  setGrid: (grid: boolean) => void
  setGridSize: (gridSize: number) => void
  setSnapping: (snapping: boolean) => void
  toggleSnapping: () => void
  setGuides: (guides: CanvasGuide[]) => void
  clearGuides: () => void
}

// View aids never enter the scene, renderer, or export. Only app preferences persist.
export const useCanvasViewStore = create<CanvasViewStore>()(
  persist(
    (set) => ({
      rulers: false,
      grid: false,
      gridSize: 32,
      snapping: true,
      guides: [],
      setRulers: (rulers) => set({ rulers }),
      setGrid: (grid) => set({ grid }),
      setGridSize: (gridSize) => {
        if (Number.isFinite(gridSize))
          set({ gridSize: Math.max(1, Math.min(4096, Math.round(gridSize))) })
      },
      setSnapping: (snapping) => set({ snapping }),
      toggleSnapping: () => set((state) => ({ snapping: !state.snapping })),
      setGuides: (guides) => set({ guides }),
      clearGuides: () => set({ guides: [] }),
    }),
    {
      name: 'koharu-canvas-view',
      partialize: ({ rulers, grid, gridSize, snapping }) => ({ rulers, grid, gridSize, snapping }),
      merge: (saved, current) => {
        const preferences = saved as Partial<CanvasViewPreferences> | null
        return {
          ...current,
          rulers: typeof preferences?.rulers === 'boolean' ? preferences.rulers : current.rulers,
          grid: typeof preferences?.grid === 'boolean' ? preferences.grid : current.grid,
          snapping:
            typeof preferences?.snapping === 'boolean' ? preferences.snapping : current.snapping,
          gridSize:
            typeof preferences?.gridSize === 'number' && Number.isFinite(preferences.gridSize)
              ? Math.max(1, Math.min(4096, Math.round(preferences.gridSize)))
              : current.gridSize,
        }
      },
    },
  ),
)
