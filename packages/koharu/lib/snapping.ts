import type { EntityId, Frame, Point, TransformFrame } from '@koharu/bridge/protocol'

import { resizeFrame, type ResizeHandle } from './geometry'

export interface SnapGuide {
  axis: 'x' | 'y'
  position: number
}

export interface SnapLine extends SnapGuide {
  source: 'bubble' | 'guide' | 'grid'
}

export interface SnapOptions {
  /** CSS pixels per page pixel, independent of the display's pixel density. */
  zoom: number
  enabled: boolean
  bypass?: boolean
  gridSize?: number | null
  guides?: readonly SnapGuide[]
  bubbleCenter?: Point | null
  /** Capture distance in CSS pixels. */
  threshold?: number
}

interface SnapMatch {
  delta: number
  line: SnapLine
}

interface SnapResult {
  frame: Frame
  lines: SnapLine[]
}

const priority = { bubble: 0, guide: 1, grid: 2 }

export function snapMoveFrame(frame: Frame, options: SnapOptions): SnapResult {
  const matches = moveMatches(frame, options)
  return {
    frame: {
      ...frame,
      x: frame.x + (matches.x?.delta ?? 0),
      y: frame.y + (matches.y?.delta ?? 0),
    },
    lines: matchLines(matches),
  }
}

export function snapMoveFrames(
  frames: readonly TransformFrame[],
  options: SnapOptions,
  bubbleCenters: Readonly<Record<EntityId, Point>> = {},
): { frames: TransformFrame[]; lines: SnapLine[] } {
  const matches: Partial<Record<'x' | 'y', SnapMatch>> = {}
  for (const { element, frame } of frames) {
    const candidate = moveMatches(frame, {
      ...options,
      bubbleCenter: bubbleCenters[element] ?? options.bubbleCenter,
    })
    for (const axis of ['x', 'y'] as const) {
      if (candidate[axis] && better(candidate[axis], matches[axis])) matches[axis] = candidate[axis]
    }
  }
  return {
    frames: frames.map(({ element, frame }) => ({
      element,
      frame: {
        ...frame,
        x: frame.x + (matches.x?.delta ?? 0),
        y: frame.y + (matches.y?.delta ?? 0),
      },
    })),
    lines: matchLines(matches),
  }
}

export function snapResizeFrame(
  original: Frame,
  handle: ResizeHandle,
  point: Point,
  minimumSize: number,
  options: SnapOptions,
): SnapResult {
  const frame = resizeFrame(original, handle, point, minimumSize)
  const threshold = pageThreshold(options)
  if (threshold === null) return { frame, lines: [] }
  const position = handlePoint(frame, handle)
  const corner = handle.length === 2
  const matches: Partial<Record<'x' | 'y', SnapMatch>> = {}
  let target = { ...position }

  if (corner) {
    for (const axis of ['x', 'y'] as const) {
      matches[axis] = axisMatch([position[axis]], axis, options, threshold)
      target[axis] += matches[axis]?.delta ?? 0
    }
  } else {
    const angle = (frame.angle_degrees * Math.PI) / 180
    const direction =
      handle === 'e' || handle === 'w'
        ? { x: Math.cos(angle), y: Math.sin(angle) }
        : { x: -Math.sin(angle), y: Math.cos(angle) }
    let closest: { distance: number; match: SnapMatch } | undefined
    for (const axis of ['x', 'y'] as const) {
      if (Math.abs(direction[axis]) < 1e-9) continue
      const match = axisMatch(
        [position[axis]],
        axis,
        options,
        threshold * Math.abs(direction[axis]),
      )
      if (!match) continue
      const distance = match.delta / direction[axis]
      if (
        !closest ||
        better({ ...match, delta: distance }, { ...closest.match, delta: closest.distance })
      ) {
        closest = { distance, match }
      }
    }
    if (closest) {
      target = {
        x: position.x + direction.x * closest.distance,
        y: position.y + direction.y * closest.distance,
      }
      matches[closest.match.line.axis] = closest.match
    }
  }

  const snapped = resizeFrame(original, handle, target, minimumSize)
  const actual = handlePoint(snapped, handle)
  // Minimum dimensions can stop the handle short of a target; do not advertise a false snap.
  const lines = matchLines(matches).filter(
    (line) => Math.abs(actual[line.axis] - line.position) < 1e-6,
  )
  return { frame: snapped, lines }
}

export function snapRotation(
  angle: number,
  { enabled, shift, bypass = false }: { enabled: boolean; shift: boolean; bypass?: boolean },
): number {
  if (bypass || !Number.isFinite(angle)) return angle
  if (shift) return normalizeDegrees(Math.round(angle / 15) * 15)
  const cardinal = Math.round(angle / 90) * 90
  return enabled && Math.abs(angle - cardinal) <= 4 ? normalizeDegrees(cardinal) : angle
}

function moveMatches(frame: Frame, options: SnapOptions): Partial<Record<'x' | 'y', SnapMatch>> {
  const threshold = pageThreshold(options)
  if (threshold === null) return {}
  const angle = (frame.angle_degrees * Math.PI) / 180
  const width = Math.abs(Math.cos(angle)) * frame.width + Math.abs(Math.sin(angle)) * frame.height
  const height = Math.abs(Math.sin(angle)) * frame.width + Math.abs(Math.cos(angle)) * frame.height
  const center = { x: frame.x + frame.width / 2, y: frame.y + frame.height / 2 }
  const matches: Partial<Record<'x' | 'y', SnapMatch>> = {}
  for (const axis of ['x', 'y'] as const) {
    const half = (axis === 'x' ? width : height) / 2
    matches[axis] = axisMatch(
      [center[axis] - half, center[axis], center[axis] + half],
      axis,
      options,
      threshold,
    )
    const position = options.bubbleCenter?.[axis]
    if (position !== undefined && Number.isFinite(position)) {
      const bubble: SnapMatch = {
        delta: position - center[axis],
        line: { axis, position, source: 'bubble' },
      }
      if (Math.abs(bubble.delta) <= threshold && better(bubble, matches[axis]))
        matches[axis] = bubble
    }
  }
  return matches
}

function axisMatch(
  anchors: number[],
  axis: 'x' | 'y',
  options: SnapOptions,
  threshold: number,
): SnapMatch | undefined {
  let best: SnapMatch | undefined
  const consider = (anchor: number, position: number, source: SnapLine['source']) => {
    if (!Number.isFinite(position) || !Number.isFinite(anchor)) return
    const candidate = { delta: position - anchor, line: { axis, position, source } }
    if (Math.abs(candidate.delta) <= threshold && better(candidate, best)) best = candidate
  }
  for (const anchor of anchors) {
    for (const guide of options.guides ?? []) {
      if (guide.axis === axis) consider(anchor, guide.position, 'guide')
    }
    const size = options.gridSize
    if (size && Number.isFinite(size) && size > 0) {
      consider(anchor, Math.round(anchor / size) * size, 'grid')
    }
  }
  return best
}

function better(candidate: SnapMatch, current: SnapMatch | undefined): boolean {
  if (!current) return true
  const difference = Math.abs(candidate.delta) - Math.abs(current.delta)
  return (
    difference < -1e-9 ||
    (Math.abs(difference) <= 1e-9 &&
      priority[candidate.line.source] < priority[current.line.source])
  )
}

function matchLines(matches: Partial<Record<'x' | 'y', SnapMatch>>): SnapLine[] {
  return [matches.x?.line, matches.y?.line].filter((line): line is SnapLine => Boolean(line))
}

function pageThreshold(options: SnapOptions): number | null {
  if (!options.enabled || options.bypass || !Number.isFinite(options.zoom) || options.zoom <= 0)
    return null
  const threshold = options.threshold ?? 6
  return Number.isFinite(threshold) && threshold >= 0 ? threshold / options.zoom : null
}

function handlePoint(frame: Frame, handle: ResizeHandle): Point {
  const x = handle.includes('e') ? frame.width / 2 : handle.includes('w') ? -frame.width / 2 : 0
  const y = handle.includes('s') ? frame.height / 2 : handle.includes('n') ? -frame.height / 2 : 0
  const angle = (frame.angle_degrees * Math.PI) / 180
  return {
    x: frame.x + frame.width / 2 + x * Math.cos(angle) - y * Math.sin(angle),
    y: frame.y + frame.height / 2 + x * Math.sin(angle) + y * Math.cos(angle),
  }
}

function normalizeDegrees(value: number): number {
  return ((((value + 180) % 360) + 360) % 360) - 180
}
