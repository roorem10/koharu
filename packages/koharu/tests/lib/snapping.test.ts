import { describe, expect, it } from 'vitest'

import { snapMoveFrame, snapMoveFrames, snapResizeFrame, snapRotation } from '@/lib/snapping'
import type { Frame } from '@koharu/bridge/protocol'

const frame: Frame = { x: 13, y: 24, width: 20, height: 20, angle_degrees: 0 }
const enabled = { enabled: true, zoom: 1 }

describe('canvas snapping', () => {
  it('snaps to the closest grid line without changing dimensions', () => {
    const result = snapMoveFrame(frame, { ...enabled, gridSize: 10 })
    expect(result.frame).toEqual({ ...frame, x: 10, y: 20 })
    expect(result.lines).toEqual([
      { axis: 'x', position: 10, source: 'grid' },
      { axis: 'y', position: 20, source: 'grid' },
    ])
  })

  it('snaps edges to guides and prefers a guide over an equally close grid line', () => {
    const result = snapMoveFrame(frame, {
      ...enabled,
      gridSize: 10,
      guides: [
        { axis: 'x', position: 16 },
        { axis: 'y', position: 45 },
      ],
    })
    expect(result.frame).toEqual({ ...frame, x: 16, y: 25 })
    expect(result.lines.every((line) => line.source === 'guide')).toBe(true)
  })

  it('aligns only the text center to its bubble and prefers it on a tie', () => {
    const result = snapMoveFrame(frame, {
      ...enabled,
      gridSize: 10,
      guides: [{ axis: 'x', position: 16 }],
      bubbleCenter: { x: 26, y: 35 },
    })
    expect(result.frame).toEqual({ ...frame, x: 16, y: 25 })
    expect(result.lines).toEqual([
      { axis: 'x', position: 26, source: 'bubble' },
      { axis: 'y', position: 35, source: 'bubble' },
    ])
    expect(snapMoveFrame(frame, { ...enabled, bubbleCenter: { x: 13, y: 24 } }).lines).toEqual([])
  })

  it('keeps capture distance constant on screen as zoom changes', () => {
    const guides = [{ axis: 'x' as const, position: 18 }]
    expect(snapMoveFrame(frame, { ...enabled, zoom: 1, guides }).frame.x).toBe(18)
    expect(snapMoveFrame(frame, { ...enabled, zoom: 2, guides }).frame.x).toBe(13)
    expect(snapMoveFrame(frame, { ...enabled, zoom: 0.5, guides }).frame.x).toBe(18)
  })

  it('preserves group spacing when different selected boxes supply each snap axis', () => {
    const second = { ...frame, x: 63, y: 72 }
    const result = snapMoveFrames(
      [
        { element: 'first', frame },
        { element: 'second', frame: second },
      ],
      {
        ...enabled,
        guides: [
          { axis: 'x', position: 14 },
          { axis: 'y', position: 90 },
        ],
      },
    )
    expect(result.frames[0].frame).toEqual({ ...frame, x: 14, y: 22 })
    expect(result.frames[1].frame).toEqual({ ...second, x: 64, y: 70 })
  })

  it('uses the rotated visible bounds when moving a box', () => {
    const result = snapMoveFrame(
      { ...frame, width: 40, angle_degrees: 90 },
      { ...enabled, guides: [{ axis: 'x', position: 25 }] },
    )
    expect(result.frame.x).toBeCloseTo(15)
    expect(result.frame.angle_degrees).toBe(90)
  })

  it('resizes to grid and guide lines while preserving the opposite corner', () => {
    const result = snapResizeFrame(frame, 'nw', { x: 8, y: 21 }, 1, {
      ...enabled,
      gridSize: 10,
      guides: [{ axis: 'y', position: 22 }],
    })
    expect(result.frame).toEqual({ ...frame, x: 10, y: 22, width: 23, height: 22 })
    expect(result.lines).toEqual([
      { axis: 'x', position: 10, source: 'grid' },
      { axis: 'y', position: 22, source: 'guide' },
    ])
  })

  it('snaps a rotated edge along its own axis without moving the fixed edge', () => {
    const rotated = { ...frame, width: 40, angle_degrees: 45 }
    const center = { x: 33, y: 34 }
    const point = { x: center.x + 24 / Math.SQRT2, y: center.y + 24 / Math.SQRT2 }
    const result = snapResizeFrame(rotated, 'e', point, 1, {
      ...enabled,
      guides: [{ axis: 'x', position: 52 }],
    })
    const nextCenter = {
      x: result.frame.x + result.frame.width / 2,
      y: result.frame.y + result.frame.height / 2,
    }
    expect(nextCenter.x + result.frame.width / 2 / Math.SQRT2).toBeCloseTo(52)
    expect(nextCenter.x - result.frame.width / 2 / Math.SQRT2).toBeCloseTo(
      center.x - 20 / Math.SQRT2,
    )
    expect(nextCenter.y - result.frame.width / 2 / Math.SQRT2).toBeCloseTo(
      center.y - 20 / Math.SQRT2,
    )
    expect(result.frame.height).toBe(20)
    expect(result.lines).toEqual([{ axis: 'x', position: 52, source: 'guide' }])
  })

  it('does not report a snap prevented by the minimum box size', () => {
    const result = snapResizeFrame(frame, 'w', { x: 31, y: 34 }, 5, {
      ...enabled,
      guides: [{ axis: 'x', position: 30 }],
    })
    expect(result.frame.width).toBe(5)
    expect(result.lines).toEqual([])
  })

  it('bypasses every positional source and ignores invalid grid sizes', () => {
    const options = {
      ...enabled,
      gridSize: 10,
      guides: [{ axis: 'x' as const, position: 16 }],
      bubbleCenter: { x: 26, y: 35 },
    }
    expect(snapMoveFrame(frame, { ...options, bypass: true })).toEqual({ frame, lines: [] })
    expect(snapMoveFrame(frame, { ...options, enabled: false })).toEqual({ frame, lines: [] })
    for (const gridSize of [0, -10, Infinity, NaN]) {
      expect(snapMoveFrame(frame, { ...enabled, gridSize })).toEqual({ frame, lines: [] })
    }
  })

  it('constrains rotation with Shift independently of the snapping toggle', () => {
    expect(snapRotation(22, { enabled: false, shift: true })).toBe(15)
    expect(snapRotation(-23, { enabled: true, shift: true })).toBe(-30)
    expect(snapRotation(22, { enabled: true, shift: true, bypass: true })).toBe(22)
  })

  it('captures only nearby cardinal rotations, including across the angle wrap', () => {
    for (const [angle, expected] of [
      [3, 0],
      [87, 90],
      [177, -180],
      [-92, -90],
      [358, 0],
    ]) {
      expect(snapRotation(angle, { enabled: true, shift: false })).toBe(expected)
    }
    expect(snapRotation(84, { enabled: true, shift: false })).toBe(84)
    expect(snapRotation(87, { enabled: false, shift: false })).toBe(87)
  })
})
