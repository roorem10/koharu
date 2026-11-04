'use client'

import { useRef, useState, type PointerEvent as ReactPointerEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { originalVisible, useCompareStore } from '@/components/qc/compare'
import {
  cornerPagePoint,
  pointCorner,
  validCorners,
  warpCorners,
  type Corners,
} from '@/components/warp/geometry'
import { useTextWarp } from '@/components/warp/useTextWarp'
import { effectiveLayerVisibility } from '@/lib/document'
import { controlFrame, type Camera } from '@/lib/geometry'
import { usePage } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import type { EntityId, Frame, TextWarp } from '@koharu/bridge/protocol'

export function WarpHandles({ layerId }: { layerId: EntityId }) {
  const page = usePage().data
  const camera = useKoharuStore((state) => state.camera)
  const frames = useKoharuStore((state) => state.layerFrames)
  const revision = useKoharuStore((state) => state.canvasRevision)
  const showingOriginal = useCompareStore((state) => state.pinned || state.held)
  const { warps, ready, busy, setWarp } = useTextWarp([layerId])
  const layer = page?.layers.find((layer) => layer.id === layerId)
  const warp = warps?.[0]
  const frame = layer && controlFrame(layer, frames)
  const visibility = layer && page && effectiveLayerVisibility(page.layers, layer)
  if (
    !ready ||
    !warp ||
    !frame ||
    showingOriginal ||
    !visibility?.visible ||
    visibility.opacity <= 0
  )
    return null
  return (
    <CornerControls
      key={JSON.stringify([page?.id, layerId, revision, warp, frame, camera])}
      warp={warp}
      frame={frame}
      camera={camera}
      disabled={busy}
      setWarp={setWarp}
    />
  )
}

type Gesture = {
  pointer: number
  corner: number
  x: number
  y: number
  corners: Corners
}

function CornerControls({
  warp,
  frame,
  camera,
  disabled,
  setWarp,
}: {
  warp: TextWarp
  frame: Frame
  camera: Camera
  disabled: boolean
  setWarp: (warp: TextWarp) => void
}) {
  const { t } = useTranslation()
  const gesture = useRef<Gesture | null>(null)
  const [preview, setPreview] = useState<Corners | null>(null)
  const corners = preview ?? warpCorners(warp, frame)
  const scale = camera.zoom / window.devicePixelRatio
  const points = corners.map((corner) => {
    const point = cornerPagePoint(corner, frame)
    return {
      x: point.x * scale + camera.translation[0] / window.devicePixelRatio,
      y: point.y * scale + camera.translation[1] / window.devicePixelRatio,
    }
  })
  const update = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const current = gesture.current
    if (!current || event.pointerId !== current.pointer || warp.kind !== 'perspective') return null
    const original = cornerPagePoint(warp.corners[current.corner], frame)
    const next = warp.corners.map((corner) => [...corner]) as Corners
    next[current.corner] = pointCorner(
      {
        x: original.x + (event.clientX - current.x) / scale,
        y: original.y + (event.clientY - current.y) / scale,
      },
      frame,
    ).map(Math.fround) as [number, number]
    if (validCorners(next)) current.corners = next
    return current.corners
  }
  const cancel = () => {
    gesture.current = null
    setPreview(null)
  }
  return (
    <div data-testid='warp-outline' className='pointer-events-none absolute inset-0 z-20'>
      <svg aria-hidden className='absolute inset-0 size-full overflow-visible'>
        <polygon
          points={points.map((point) => `${point.x},${point.y}`).join(' ')}
          fill='none'
          stroke='var(--canvas-selection)'
          strokeWidth='1.5'
          vectorEffect='non-scaling-stroke'
        />
      </svg>
      {warp.kind === 'perspective' &&
        points.map((point, index) => (
          <button
            key={index}
            type='button'
            data-canvas-control
            data-warp-corner={index}
            aria-label={t('warp.corner', { number: index + 1 })}
            disabled={disabled}
            className='pointer-events-auto absolute size-3 -translate-x-1/2 -translate-y-1/2 cursor-move touch-none rounded-full border-2 border-primary-foreground bg-[var(--canvas-selection)] shadow-sm disabled:pointer-events-none'
            style={{ left: point.x, top: point.y }}
            onPointerDown={(event) => {
              if (event.button !== 0 || disabled || gesture.current || originalVisible()) return
              event.preventDefault()
              event.stopPropagation()
              event.currentTarget.setPointerCapture(event.pointerId)
              gesture.current = {
                pointer: event.pointerId,
                corner: index,
                x: event.clientX,
                y: event.clientY,
                corners: warp.corners,
              }
            }}
            onPointerMove={(event) => {
              if (!gesture.current || gesture.current.pointer !== event.pointerId) return
              event.preventDefault()
              event.stopPropagation()
              if (originalVisible()) {
                cancel()
                return
              }
              const next = update(event)
              if (next) setPreview(next)
            }}
            onPointerUp={(event) => {
              if (!gesture.current || gesture.current.pointer !== event.pointerId) return
              event.preventDefault()
              event.stopPropagation()
              const next = update(event)
              cancel()
              if (event.currentTarget.hasPointerCapture(event.pointerId))
                event.currentTarget.releasePointerCapture(event.pointerId)
              if (
                next &&
                !originalVisible() &&
                JSON.stringify(next) !== JSON.stringify(warp.corners)
              )
                setWarp({ kind: 'perspective', corners: next })
            }}
            onPointerCancel={(event) => {
              event.stopPropagation()
              cancel()
            }}
            onLostPointerCapture={cancel}
          />
        ))}
    </div>
  )
}
