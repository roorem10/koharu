import type { ReactNode } from 'react'

import type { Point } from '@koharu/bridge/protocol'

/** A canvas tool that handles its own pointer gestures; points are in page pixels. */
export interface CanvasToolHandler {
  /** Return true to take the gesture; the built-in tool code then ignores it. */
  start: (point: Point, event: PointerEvent) => boolean
  move: (point: Point) => void
  /** `event` is the pointer-up event, or null when the gesture was cancelled. */
  finish: (event: PointerEvent | null) => void
  /** Drawn above the page while the tool is active. */
  overlay: ReactNode
}
