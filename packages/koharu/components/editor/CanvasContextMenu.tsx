'use client'

import type { ReactNode } from 'react'

import type { Point } from '@koharu/bridge/protocol'

// Owned by WO-23: the canvas right-click menu (FX…, Box details, Split, Join with bubble).
// `open` gets the right-click event and the page point under the pointer.
export function useCanvasContextMenu(): {
  open: (event: MouseEvent, point: Point) => void
  menu: ReactNode
} {
  return { open: () => {}, menu: null }
}
