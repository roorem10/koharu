'use client'

import type { ReactNode } from 'react'

import type { EntityId, Point } from '@koharu/bridge/protocol'

/**
 * WO-11D: the on-page editor. `CanvasWorkspace` calls `open` when a text box is
 * double-clicked (not while the original page is showing) and renders `overlay`
 * above the page. `open` returns false when it doesn't take the gesture, so the
 * old double-click behaviour runs. `editing` is the layer in edit mode, if any:
 * while it is set, `CanvasWorkspace` leaves pointer gestures inside that box to
 * the editor's overlay.
 */
export function useRichTextEditor(): {
  open: (layer: EntityId, point: Point, event: MouseEvent) => boolean
  editing: EntityId | null
  overlay: ReactNode
} {
  return { open: () => false, editing: null, overlay: null }
}
