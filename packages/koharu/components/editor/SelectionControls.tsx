'use client'

import { useEffect, useRef, type PointerEvent as ReactPointerEvent, type RefObject } from 'react'

import { useCanvasViewStore } from '@/components/editor/viewAids'
import { cssFrame, pagePoint, rotateFrame, type Camera, type ResizeHandle } from '@/lib/geometry'
import { snapResizeFrame, snapRotation, type SnapLine } from '@/lib/snapping'
import type { EntityId, Frame, Point, TransformFrame } from '@koharu/bridge/protocol'

interface SelectionControlsProps {
  container: RefObject<HTMLDivElement | null>
  element: EntityId
  frame: Frame
  camera: Camera
  edgesOnly: boolean
  snappingAllowed: boolean
  onSnapLines: (lines: SnapLine[]) => void
  onTransformStart: (elements: TransformFrame[]) => void
  onTransformFrame: (elements: TransformFrame[]) => void
  onTransformEnd: () => void
  onTransformCancel: () => void
}

type ControlGesture =
  | {
      kind: 'resize'
      pointer: number
      original: Frame
      handle: ResizeHandle
      start: Point
      dragging: boolean
    }
  | { kind: 'rotate'; pointer: number; original: Frame; start: Point; dragging: boolean }

const handles: Array<{
  handle: ResizeHandle
  left: string
  top: string
  cursor: string
}> = [
  { handle: 'nw', left: '0%', top: '0%', cursor: 'nwse-resize' },
  { handle: 'n', left: '50%', top: '0%', cursor: 'ns-resize' },
  { handle: 'ne', left: '100%', top: '0%', cursor: 'nesw-resize' },
  { handle: 'e', left: '100%', top: '50%', cursor: 'ew-resize' },
  { handle: 'se', left: '100%', top: '100%', cursor: 'nwse-resize' },
  { handle: 's', left: '50%', top: '100%', cursor: 'ns-resize' },
  { handle: 'sw', left: '0%', top: '100%', cursor: 'nesw-resize' },
  { handle: 'w', left: '0%', top: '50%', cursor: 'ew-resize' },
]

export function SelectionControls({
  container,
  element,
  frame,
  camera,
  edgesOnly,
  snappingAllowed,
  onSnapLines,
  onTransformStart,
  onTransformFrame,
  onTransformEnd,
  onTransformCancel,
}: SelectionControlsProps) {
  const gesture = useRef<ControlGesture | null>(null)
  const position = cssFrame(frame, camera)
  const outside = position.width < 40 || position.height < 40

  useEffect(() => {
    const cancel = () => {
      if (!gesture.current) return
      gesture.current = null
      onTransformCancel()
    }
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') cancel()
    }
    window.addEventListener('keydown', key)
    window.addEventListener('blur', cancel)
    return () => {
      window.removeEventListener('keydown', key)
      window.removeEventListener('blur', cancel)
      cancel()
    }
  }, [onTransformCancel])

  const eventPoint = (event: ReactPointerEvent<HTMLDivElement>): Point | null => {
    const bounds = container.current?.getBoundingClientRect()
    return bounds ? pagePoint(event.clientX, event.clientY, bounds, camera) : null
  }

  const capture = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    event.stopPropagation()
    event.currentTarget.setPointerCapture(event.pointerId)
  }

  const startResize = (handle: ResizeHandle) => (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || gesture.current) return
    const start = eventPoint(event)
    if (!start) return
    capture(event)
    gesture.current = {
      kind: 'resize',
      pointer: event.pointerId,
      original: frame,
      handle,
      start,
      dragging: false,
    }
    onTransformStart([{ element, frame }])
  }

  const startRotate = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || gesture.current) return
    const start = eventPoint(event)
    if (!start) return
    capture(event)
    gesture.current = {
      kind: 'rotate',
      pointer: event.pointerId,
      original: frame,
      start,
      dragging: false,
    }
    onTransformStart([{ element, frame }])
  }

  const update = (event: ReactPointerEvent<HTMLDivElement>) => {
    const current = gesture.current
    if (!current || current.pointer !== event.pointerId) return
    event.preventDefault()
    event.stopPropagation()
    const point = eventPoint(event)
    if (!point) return
    if (
      !current.dragging &&
      (Math.hypot(point.x - current.start.x, point.y - current.start.y) * camera.zoom) /
        window.devicePixelRatio <
        3
    )
      return
    current.dragging = true
    const aids = useCanvasViewStore.getState()
    const bypass = event.ctrlKey || event.metaKey
    let next: Frame
    if (current.kind === 'resize') {
      const snapped = snapResizeFrame(
        current.original,
        current.handle,
        point,
        window.devicePixelRatio / camera.zoom,
        {
          zoom: camera.zoom / window.devicePixelRatio,
          enabled: snappingAllowed && aids.snapping,
          bypass,
          gridSize: aids.grid ? aids.gridSize : null,
          guides: aids.guides,
        },
      )
      next = snapped.frame
      onSnapLines(snapped.lines)
    } else {
      next = rotateFrame(current.original, current.start, point)
      if (snappingAllowed)
        next.angle_degrees = snapRotation(next.angle_degrees, {
          enabled: aids.snapping,
          shift: event.shiftKey,
          bypass,
        })
      onSnapLines([])
    }
    onTransformFrame([{ element, frame: next }])
  }

  const finish = (event: ReactPointerEvent<HTMLDivElement>) => {
    const current = gesture.current
    if (!current || current.pointer !== event.pointerId) return
    event.preventDefault()
    event.stopPropagation()
    update(event)
    gesture.current = null
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
    if (current.dragging) onTransformEnd()
    else onTransformCancel()
  }

  const lostCapture = (event: ReactPointerEvent<HTMLDivElement>) => {
    const current = gesture.current
    if (!current || current.pointer !== event.pointerId) return
    event.stopPropagation()
    gesture.current = null
    onTransformCancel()
  }

  const pointerEvents = {
    onPointerMove: update,
    onPointerUp: finish,
    onPointerCancel: lostCapture,
    onLostPointerCapture: lostCapture,
  }

  return (
    <div
      data-testid='selection-controls'
      className='pointer-events-none absolute box-border border border-[var(--canvas-selection)]'
      style={{
        left: position.left,
        top: position.top,
        width: position.width,
        height: position.height,
        transform: `rotate(${position.angle}deg)`,
        transformOrigin: '50% 50%',
      }}
    >
      {handles.map(({ handle, left, top, cursor }) => (
        <div
          key={handle}
          data-canvas-control
          data-resize-handle={handle}
          className='pointer-events-auto absolute grid size-3.5 touch-none place-items-center'
          style={{
            left,
            top,
            cursor,
            // Keep the whole centre available to the canvas move gesture.
            transform: `translate(${outside && left !== '50%' ? (left === '0%' ? '-100%' : '0%') : '-50%'}, ${outside && top !== '50%' ? (top === '0%' ? '-100%' : '0%') : '-50%'})`,
            width:
              edgesOnly && (handle === 'n' || handle === 's')
                ? 'max(14px, calc(100% - 14px))'
                : undefined,
            height:
              edgesOnly && (handle === 'e' || handle === 'w')
                ? 'max(14px, calc(100% - 14px))'
                : undefined,
          }}
          onPointerDown={startResize(handle)}
          {...pointerEvents}
        >
          {!edgesOnly && (
            <span className='size-1.5 rounded-[1px] border border-primary-foreground bg-[var(--canvas-selection)] shadow-[0_0_0_1px_rgb(0_0_0/0.12)]' />
          )}
        </div>
      ))}

      <span className='pointer-events-none absolute top-0 left-1/2 h-2 w-px -translate-x-1/2 -translate-y-full bg-[var(--canvas-selection)]' />
      <div
        data-canvas-control
        data-rotate-handle
        className='pointer-events-auto absolute top-[-16px] left-1/2 grid size-3.5 -translate-x-1/2 -translate-y-1/2 cursor-alias touch-none place-items-center'
        onPointerDown={startRotate}
        {...pointerEvents}
      >
        <span className='size-1.5 rounded-[1px] border border-primary-foreground bg-[var(--canvas-selection)] shadow-[0_0_0_1px_rgb(0_0_0/0.12)]' />
      </div>
    </div>
  )
}
