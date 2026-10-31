'use client'

import { useMutation } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'

import {
  ocrBoxEditKey,
  refreshOcrBoxes,
  useOcrBoxBusy,
  useOcrBoxSettings,
} from '@/components/ocr-box/queries'
import { originalVisible } from '@/components/qc/compare'
import { call } from '@/lib/backend'
import { usePage } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type Bounds,
  type EntityId,
  type OcrBoxMode,
  type Point,
} from '@koharu/bridge/protocol'

import type { CanvasToolHandler } from './types'

type Drag = { page: EntityId; start: Point; end: Point; size: { width: number; height: number } }

export function useOcrBoxTool(): CanvasToolHandler {
  const page = usePage().data
  const settings = useOcrBoxSettings()
  const busy = useOcrBoxBusy()
  const camera = useKoharuStore((state) => state.camera)
  const tool = useKoharuStore((state) => state.tool)
  const [preview, setPreview] = useState<Bounds | null>(null)
  const drag = useRef<Drag | null>(null)
  const current = useRef({ page, settings: settings.data, busy })
  current.current = { page, settings: settings.data, busy }
  const create = useMutation({
    mutationKey: ocrBoxEditKey,
    mutationFn: async ({
      page,
      rect,
      mode,
    }: {
      page: EntityId
      rect: Bounds
      mode: OcrBoxMode
    }) => {
      const layer = await call(commands.addOcrBox, page, rect, mode)
      if (current.current.page?.id === page && useKoharuStore.getState().canvasPage === page) {
        useKoharuStore.getState().selectLayers([layer])
      }
    },
    onSettled: refreshOcrBoxes,
  })
  useEffect(() => {
    drag.current = null
    setPreview(null)
  }, [page?.id, tool])

  const frame = (gesture: Drag): Bounds => ({
    x: Math.min(gesture.start.x, gesture.end.x),
    y: Math.min(gesture.start.y, gesture.end.y),
    width: Math.abs(gesture.end.x - gesture.start.x),
    height: Math.abs(gesture.end.y - gesture.start.y),
  })
  const clamp = (point: Point, size: Drag['size']): Point => ({
    x: Math.max(0, Math.min(point.x, size.width)),
    y: Math.max(0, Math.min(point.y, size.height)),
  })
  return {
    start: (point, event) => {
      const { page, settings, busy } = current.current
      if (
        !page ||
        !settings ||
        busy ||
        originalVisible() ||
        event.button !== 0 ||
        useKoharuStore.getState().tool !== 'ocr_box' ||
        !Number.isFinite(point.x) ||
        !Number.isFinite(point.y)
      )
        return false
      const start = clamp(point, page.size)
      drag.current = { page: page.id, size: page.size, start, end: start }
      setPreview(frame(drag.current))
      return true
    },
    move: (point) => {
      const gesture = drag.current
      if (!gesture || !Number.isFinite(point.x) || !Number.isFinite(point.y)) return
      gesture.end = clamp(point, gesture.size)
      setPreview(frame(gesture))
    },
    finish: (event) => {
      const gesture = drag.current
      drag.current = null
      setPreview(null)
      if (
        !gesture ||
        !event ||
        current.current.busy ||
        originalVisible() ||
        current.current.page?.id !== gesture.page ||
        useKoharuStore.getState().tool !== 'ocr_box'
      )
        return
      const rect = frame(gesture)
      const scale = useKoharuStore.getState().camera.zoom / window.devicePixelRatio
      if (rect.width * scale < 4 || rect.height * scale < 4) return
      const mode = event.altKey
        ? 'read'
        : event.shiftKey
          ? 'read_translate_clean'
          : (current.current.settings?.default_mode ?? 'read_translate')
      create.mutate({ page: gesture.page, rect, mode })
    },
    overlay: preview ? (
      <div
        data-testid='ocr-box-preview'
        className='pointer-events-none absolute z-10 border border-dashed border-[var(--canvas-selection)] bg-[var(--canvas-selection)]/10'
        style={{
          left: (preview.x * camera.zoom + camera.translation[0]) / window.devicePixelRatio,
          top: (preview.y * camera.zoom + camera.translation[1]) / window.devicePixelRatio,
          width: (preview.width * camera.zoom) / window.devicePixelRatio,
          height: (preview.height * camera.zoom) / window.devicePixelRatio,
        }}
      />
    ) : null,
  }
}
