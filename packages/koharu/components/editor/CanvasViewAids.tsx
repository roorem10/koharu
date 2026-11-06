'use client'

import {
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
  type PointerEvent as ReactPointerEvent,
} from 'react'
import { useTranslation } from 'react-i18next'

import { useCanvasViewStore, type CanvasGuide } from '@/components/editor/viewAids'
import { pagePoint } from '@/lib/geometry'
import { useKoharuStore } from '@/lib/store'
import type { PageSize } from '@koharu/bridge/protocol'

const rulerSize = 22
type GuideLine = Pick<CanvasGuide, 'axis' | 'position'>
interface GuideDrag {
  pointer: number
  guide: CanvasGuide
  grabOffset: number
}

export function CanvasViewAids({
  size,
  disabled = false,
  snapLines = [],
}: {
  size: PageSize
  disabled?: boolean
  snapLines?: readonly GuideLine[]
}) {
  const { t } = useTranslation()
  const root = useRef<HTMLDivElement>(null)
  const drag = useRef<GuideDrag | null>(null)
  const [draft, setDraft] = useState<CanvasGuide | null>(null)
  const [viewport, setViewport] = useState({ width: 0, height: 0 })
  const pattern = useId().replaceAll(':', '')
  const camera = useKoharuStore((state) => state.camera)
  const tool = useKoharuStore((state) => state.tool)
  const view = useCanvasViewStore()
  const dpr = window.devicePixelRatio
  const scale = camera.zoom / dpr
  const left = camera.translation[0] / dpr
  const top = camera.translation[1] / dpr
  const inset = view.rulers ? rulerSize : 0

  useEffect(() => {
    const element = root.current
    if (!element) return
    const measure = () => {
      const bounds = element.getBoundingClientRect()
      setViewport({ width: bounds.width, height: bounds.height })
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    const cancel = () => {
      drag.current = null
      setDraft(null)
    }
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && drag.current) {
        event.preventDefault()
        event.stopPropagation()
        cancel()
      }
    }
    window.addEventListener('keydown', escape, true)
    window.addEventListener('blur', cancel)
    return () => {
      window.removeEventListener('keydown', escape, true)
      window.removeEventListener('blur', cancel)
    }
  }, [])

  useEffect(() => {
    drag.current = null
    setDraft(null)
  }, [disabled, tool])

  const positionAt = (event: ReactPointerEvent<HTMLDivElement>, axis: CanvasGuide['axis']) => {
    const bounds = root.current!.getBoundingClientRect()
    return pagePoint(event.clientX, event.clientY, bounds, useKoharuStore.getState().camera)[axis]
  }
  const start = (
    event: ReactPointerEvent<HTMLDivElement>,
    axis: CanvasGuide['axis'],
    existing?: CanvasGuide,
  ) => {
    if (disabled || event.button !== 0 || drag.current || (existing && tool !== 'select')) return
    event.preventDefault()
    event.stopPropagation()
    const position = positionAt(event, axis)
    const guide = existing ?? { id: crypto.randomUUID(), axis, position }
    drag.current = { pointer: event.pointerId, guide, grabOffset: position - guide.position }
    setDraft(guide)
    event.currentTarget.setPointerCapture(event.pointerId)
  }
  const move = (event: ReactPointerEvent<HTMLDivElement>) => {
    const current = drag.current
    if (!current || current.pointer !== event.pointerId) return
    event.preventDefault()
    event.stopPropagation()
    setDraft({
      ...current.guide,
      position: positionAt(event, current.guide.axis) - current.grabOffset,
    })
  }
  const cancel = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (!drag.current || drag.current.pointer !== event.pointerId) return
    event.stopPropagation()
    drag.current = null
    setDraft(null)
  }
  const finish = (event: ReactPointerEvent<HTMLDivElement>) => {
    const current = drag.current
    if (!current || current.pointer !== event.pointerId) return
    event.preventDefault()
    event.stopPropagation()
    const bounds = root.current!.getBoundingClientRect()
    const position = positionAt(event, current.guide.axis) - current.grabOffset
    const inside =
      event.clientX >= bounds.left + inset &&
      event.clientX <= bounds.right &&
      event.clientY >= bounds.top + inset &&
      event.clientY <= bounds.bottom &&
      position >= 0 &&
      position <= (current.guide.axis === 'x' ? size.width : size.height)
    const guides = useCanvasViewStore
      .getState()
      .guides.filter((guide) => guide.id !== current.guide.id)
    if (inside) guides.push({ ...current.guide, position })
    useCanvasViewStore.getState().setGuides(guides)
    drag.current = null
    setDraft(null)
    const target = event.target
    if (target instanceof Element && target.hasPointerCapture(event.pointerId))
      target.releasePointerCapture(event.pointerId)
  }

  const guides = view.guides.filter((guide) => guide.id !== draft?.id)
  if (draft) guides.push(draft)
  const gridSpacing = view.gridSize * scale

  return (
    <div
      ref={root}
      data-testid='canvas-view-aids'
      className='pointer-events-none absolute inset-0 overflow-hidden'
      onPointerMove={move}
      onPointerUp={finish}
      onPointerCancel={cancel}
      onLostPointerCapture={cancel}
    >
      <svg aria-hidden className='absolute inset-0 size-full'>
        {view.grid && gridSpacing >= 4 && (
          <>
            <defs>
              <pattern
                id={pattern}
                width={gridSpacing}
                height={gridSpacing}
                patternUnits='userSpaceOnUse'
                x={left}
                y={top}
              >
                <path
                  d={`M ${gridSpacing} 0 L 0 0 0 ${gridSpacing}`}
                  fill='none'
                  stroke='#64748b'
                  strokeWidth='1'
                  strokeOpacity='0.45'
                />
              </pattern>
            </defs>
            <rect
              x={left}
              y={top}
              width={size.width * scale}
              height={size.height * scale}
              fill={`url(#${pattern})`}
            />
          </>
        )}
        {guides.map((guide) => (
          <line
            key={guide.id}
            x1={guide.axis === 'x' ? left + guide.position * scale : inset}
            x2={guide.axis === 'x' ? left + guide.position * scale : viewport.width}
            y1={guide.axis === 'y' ? top + guide.position * scale : inset}
            y2={guide.axis === 'y' ? top + guide.position * scale : viewport.height}
            stroke='#06b6d4'
            strokeWidth='1'
          />
        ))}
        {!disabled &&
          snapLines.map((line, index) => (
            <line
              key={index}
              data-testid='snap-line'
              x1={line.axis === 'x' ? left + line.position * scale : inset}
              x2={line.axis === 'x' ? left + line.position * scale : viewport.width}
              y1={line.axis === 'y' ? top + line.position * scale : inset}
              y2={line.axis === 'y' ? top + line.position * scale : viewport.height}
              stroke='#ec4899'
              strokeWidth='1'
              strokeDasharray='5 3'
            />
          ))}
      </svg>
      {guides.map((guide) => (
        <div
          key={guide.id}
          data-canvas-control
          data-guide={guide.id}
          role='separator'
          aria-orientation={guide.axis === 'x' ? 'vertical' : 'horizontal'}
          aria-label={t(
            guide.axis === 'x' ? 'canvasAids.verticalGuide' : 'canvasAids.horizontalGuide',
          )}
          title={t('canvasAids.moveGuideHint')}
          className='absolute touch-none'
          style={
            guide.axis === 'x'
              ? {
                  left: left + guide.position * scale - 3,
                  top: inset,
                  bottom: 0,
                  width: 7,
                  cursor: 'ew-resize',
                  pointerEvents: disabled || tool !== 'select' ? 'none' : 'auto',
                }
              : {
                  top: top + guide.position * scale - 3,
                  left: inset,
                  right: 0,
                  height: 7,
                  cursor: 'ns-resize',
                  pointerEvents: disabled || tool !== 'select' ? 'none' : 'auto',
                }
          }
          onPointerDown={(event) => start(event, guide.axis, guide)}
        />
      ))}
      {view.rulers && (
        <>
          <Ruler
            axis='x'
            scale={scale}
            offset={left}
            length={viewport.width}
            disabled={disabled}
            onPointerDown={(event) => start(event, 'y')}
          />
          <Ruler
            axis='y'
            scale={scale}
            offset={top}
            length={viewport.height}
            disabled={disabled}
            onPointerDown={(event) => start(event, 'x')}
          />
          <div
            className='absolute top-0 left-0 z-20 grid place-items-center border-r border-b border-border bg-[var(--surface-toolbar)] text-[9px] text-muted-foreground'
            style={{ width: rulerSize, height: rulerSize }}
            aria-hidden
          >
            {t('canvasAids.pixelUnit')}
          </div>
        </>
      )}
    </div>
  )
}

function Ruler({
  axis,
  scale,
  offset,
  length,
  disabled,
  onPointerDown,
}: {
  axis: 'x' | 'y'
  scale: number
  offset: number
  length: number
  disabled: boolean
  onPointerDown: (event: ReactPointerEvent<HTMLDivElement>) => void
}) {
  const { t } = useTranslation()
  const target = 80 / scale
  const magnitude = 10 ** Math.floor(Math.log10(target))
  const major = ([1, 2, 5, 10].find((step) => step * magnitude >= target) ?? 10) * magnitude
  const minor = major / 5
  const start = Math.floor((rulerSize - offset) / scale / minor)
  const end = Math.ceil((length - offset) / scale / minor)
  const ticks: ReactNode[] = []
  for (let index = start; index <= end; index++) {
    const position = index * minor * scale + offset
    const labeled = index % 5 === 0
    ticks.push(
      <g
        key={index}
        transform={axis === 'x' ? `translate(${position},0)` : `translate(0,${position})`}
      >
        {axis === 'x' ? (
          <line x1='0' x2='0' y1={labeled ? 13 : 18} y2={rulerSize} />
        ) : (
          <line y1='0' y2='0' x1={labeled ? 13 : 18} x2={rulerSize} />
        )}
        {labeled && (
          <text
            stroke='none'
            fill='currentColor'
            fontSize='9'
            x='2'
            y='10'
            transform={axis === 'y' ? 'rotate(-90)' : undefined}
          >
            {Number((index * minor).toFixed(4))}
          </text>
        )}
      </g>,
    )
  }
  return (
    <div
      data-canvas-control
      data-testid={axis === 'x' ? 'horizontal-ruler' : 'vertical-ruler'}
      className='absolute top-0 left-0 z-10 touch-none overflow-hidden border-border bg-[var(--surface-toolbar)] text-muted-foreground'
      style={
        axis === 'x'
          ? {
              width: '100%',
              height: rulerSize,
              borderBottomWidth: 1,
              cursor: 'ns-resize',
              pointerEvents: disabled ? 'none' : 'auto',
            }
          : {
              height: '100%',
              width: rulerSize,
              borderRightWidth: 1,
              cursor: 'ew-resize',
              pointerEvents: disabled ? 'none' : 'auto',
            }
      }
      title={t('canvasAids.createGuideHint')}
      onPointerDown={onPointerDown}
    >
      <svg
        aria-hidden
        className='pointer-events-none size-full'
        stroke='currentColor'
        strokeWidth='1'
      >
        {ticks}
      </svg>
    </div>
  )
}
