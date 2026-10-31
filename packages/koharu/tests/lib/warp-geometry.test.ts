import { describe, expect, it } from 'vitest'

import {
  cornerPagePoint,
  pointCorner,
  unitCorners,
  validCorners,
  warpCorners,
} from '@/components/warp/geometry'

describe('warp frame coordinates', () => {
  it('round trips normalized corners through a rotated non-square frame', () => {
    const frame = { x: 40, y: 20, width: 200, height: 80, angle_degrees: 37 }
    for (const corner of [...unitCorners(), [-0.2, 1.5] as [number, number]]) {
      const result = pointCorner(cornerPagePoint(corner, frame), frame)
      expect(result[0]).toBeCloseTo(corner[0])
      expect(result[1]).toBeCloseTo(corner[1])
    }
    const skewed = warpCorners({ kind: 'skew', x_degrees: 30, y_degrees: 0 }, frame)
    expect(skewed[0][0]).toBeCloseTo((-Math.tan(Math.PI / 6) * 40) / 200)
    expect(skewed[3][0]).toBeCloseTo((Math.tan(Math.PI / 6) * 40) / 200)
  })
  it('rejects invalid quadrilaterals', () => {
    expect(validCorners(unitCorners())).toBe(true)
    for (const corners of [
      [
        [0, 0],
        [1, 1],
        [1, 0],
        [0, 1],
      ],
      [
        [0, 0],
        [0, 1],
        [1, 1],
        [1, 0],
      ],
      [
        [0, 0],
        [1, 0],
        [0.2, 0.2],
        [0, 1],
      ],
      [
        [0, 0],
        [1, 0],
        [NaN, 1],
        [0, 1],
      ],
      [
        [0, 0],
        [3, 0],
        [1, 1],
        [0, 1],
      ],
    ])
      expect(validCorners(corners as ReturnType<typeof unitCorners>)).toBe(false)
  })
})
