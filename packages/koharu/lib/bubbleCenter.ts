import type { Layer, Page, Point } from '@koharu/bridge/protocol'

/** automatic_region resolves FlowsIn before the OCR region; only bubbles are snap targets. */
export function bubbleCenter(page: Page, layer: Layer): Point | null {
  if (layer.type !== 'text' || !layer.automatic_region) return null
  const bubble = page.regions.find(
    (region) => region.id === layer.automatic_region && region.kind === 'dev.koharu.region.bubble',
  )
  const points = bubble?.geometry.points
  if (!points?.length || points.some(({ x, y }) => !Number.isFinite(x) || !Number.isFinite(y)))
    return null
  const xs = points.map(({ x }) => x)
  const ys = points.map(({ y }) => y)
  return { x: (Math.min(...xs) + Math.max(...xs)) / 2, y: (Math.min(...ys) + Math.max(...ys)) / 2 }
}
