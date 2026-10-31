'use client'

import type { CanvasToolHandler } from './types'

// Owned by WO-07: drag a box over missed text to read (and translate or clean) it.
export function useOcrBoxTool(): CanvasToolHandler {
  return { start: () => false, move: () => {}, finish: () => {}, overlay: null }
}
