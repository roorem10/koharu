import type { Frame, Point, TextWarp } from '@koharu/bridge/protocol'

export type Corners = Extract<TextWarp, { kind: 'perspective' }>['corners']

export const unitCorners = (): Corners => [
  [0, 0],
  [1, 0],
  [1, 1],
  [0, 1],
]

export function validCorners(corners: Corners): boolean {
  return corners.every(([x, y], index) => {
    const b = corners[(index + 1) % 4]
    const c = corners[(index + 2) % 4]
    return (
      Number.isFinite(x) &&
      Number.isFinite(y) &&
      x >= -1 &&
      x <= 2 &&
      y >= -1 &&
      y <= 2 &&
      (b[0] - x) * (c[1] - b[1]) - (b[1] - y) * (c[0] - b[0]) > 0
    )
  })
}

export function warpCorners(warp: TextWarp, frame: Frame): Corners {
  if (warp.kind === 'perspective') return warp.corners
  const x = Math.tan((warp.x_degrees * Math.PI) / 180)
  const y = Math.tan((warp.y_degrees * Math.PI) / 180)
  return unitCorners().map(([u, v]) => [
    u + (x * (v - 0.5) * frame.height) / frame.width,
    v + (y * (u - 0.5) * frame.width) / frame.height,
  ]) as Corners
}

export function cornerPagePoint([u, v]: [number, number], frame: Frame): Point {
  const angle = (frame.angle_degrees * Math.PI) / 180
  const x = (u - 0.5) * frame.width
  const y = (v - 0.5) * frame.height
  return {
    x: frame.x + frame.width / 2 + x * Math.cos(angle) - y * Math.sin(angle),
    y: frame.y + frame.height / 2 + x * Math.sin(angle) + y * Math.cos(angle),
  }
}

export function pointCorner(point: Point, frame: Frame): [number, number] {
  const angle = (frame.angle_degrees * Math.PI) / 180
  const x = point.x - frame.x - frame.width / 2
  const y = point.y - frame.y - frame.height / 2
  return [
    Math.max(-1, Math.min(2, (x * Math.cos(angle) + y * Math.sin(angle)) / frame.width + 0.5)),
    Math.max(-1, Math.min(2, (-x * Math.sin(angle) + y * Math.cos(angle)) / frame.height + 0.5)),
  ]
}
