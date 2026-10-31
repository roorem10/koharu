'use client'

import type { CanvasToolHandler } from './types'

// Owned by WO-15: queued and rectangle areas for the Remove tool. Returning false from
// `start` leaves the stroke to the built-in Remove brush.
export function useRemoveAreaTool(): CanvasToolHandler {
  return { start: () => false, move: () => {}, finish: () => {}, overlay: null }
}
