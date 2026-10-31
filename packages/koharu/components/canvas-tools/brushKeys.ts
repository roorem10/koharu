import { originalVisible } from '@/components/qc/compare'
import { pageKey, queryClient } from '@/lib/queries'
import { isBrushTool, maxBrushDiameter, MIN_BRUSH_DIAMETER, useKoharuStore } from '@/lib/store'
import type { Page } from '@koharu/bridge/protocol'

export function stepBrush(direction: -1 | 1) {
  const state = useKoharuStore.getState()
  if (!isBrushTool(state.tool) || originalVisible()) return
  const page = queryClient.getQueryData<Page | null>(pageKey)
  if (!page) return
  const size = state.brush.diameter
  const step = Math.max(2, Math.round(size * 0.1))
  state.setBrush({
    ...state.brush,
    diameter: Math.min(
      maxBrushDiameter(page.size),
      Math.max(MIN_BRUSH_DIAMETER, size + direction * step),
    ),
  })
}
