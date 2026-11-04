'use client'

import { useEffect, useRef, useState } from 'react'

import { AreaOverlay } from '@/components/cleanup/AreaOverlay'
import { addCleanupArea, syncCleanupProject, useCleanupQueue } from '@/components/cleanup/queue'
import { defaultCleanupSettings, useCleanupSettings } from '@/components/cleanup/settings'
import { originalVisible, useCompareStore } from '@/components/qc/compare'
import { call } from '@/lib/backend'
import { usePage, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type CleanupRect, type EntityId, type Point } from '@koharu/bridge/protocol'

import type { CanvasToolHandler } from './types'

export function useRemoveAreaTool() {
  const page = usePage().data
  const project = useProject().data?.name ?? null
  const tool = useKoharuStore((state) => state.tool)
  const comparing = useCompareStore((state) => state.held || state.pinned)
  const query = useCleanupSettings()
  const settings = query.data ?? defaultCleanupSettings
  const draft = useRef<{ page: EntityId; start: Point; current: Point } | null>(null)
  const [rect, setRect] = useState<CleanupRect | null>(null)
  useEffect(() => {
    syncCleanupProject(project)
  }, [project])
  useEffect(() => {
    draft.current = null
    setRect(null)
    useCleanupQueue.setState({ drawing: false })
  }, [page?.id, tool, comparing])
  useEffect(() => {
    const finished = () => useCleanupQueue.setState({ drawing: false })
    window.addEventListener('pointerup', finished)
    window.addEventListener('pointercancel', finished)
    window.addEventListener('blur', finished)
    return () => {
      window.removeEventListener('pointerup', finished)
      window.removeEventListener('pointercancel', finished)
      window.removeEventListener('blur', finished)
    }
  }, [])
  const rectangle = (start: Point, current: Point): CleanupRect => {
    const x = Math.max(0, Math.min(start.x, current.x))
    const y = Math.max(0, Math.min(start.y, current.y))
    return {
      x,
      y,
      width: Math.max(0, Math.min(page?.size.width ?? 0, Math.max(start.x, current.x)) - x),
      height: Math.max(0, Math.min(page?.size.height ?? 0, Math.max(start.y, current.y)) - y),
    }
  }
  const handler: CanvasToolHandler = {
    start: (point, event) => {
      if (originalVisible() || !page) return false
      useCleanupQueue.setState({ drawing: true })
      if (!event.shiftKey) return false
      draft.current = { page: page.id, start: point, current: point }
      setRect(rectangle(point, point))
      return true
    },
    move: (point) => {
      if (!draft.current) return
      draft.current.current = point
      setRect(rectangle(draft.current.start, point))
    },
    finish: (event) => {
      const current = draft.current
      draft.current = null
      setRect(null)
      useCleanupQueue.setState({ drawing: false })
      if (!event || !current || originalVisible()) return
      const area = rectangle(current.start, current.current)
      if (!area.width || !area.height) return
      void (async () => {
        const preferences = query.data ?? (await call(commands.getCleanupSettings))
        if (useCleanupQueue.getState().project !== project) return
        if (preferences.remove_mode === 'queue')
          addCleanupArea(current.page, { kind: 'rect', rect: area })
        else await call(commands.cleanAreas, current.page, [], [area])
      })().catch(() => undefined)
    },
    overlay: rect ? <AreaOverlay areas={[{ id: -1, kind: 'rect', rect }]} /> : null,
  }
  return {
    ...handler,
    settings,
    async commitStroke(
      page: EntityId,
      revision: number,
      points: Point[],
      diameter: number,
      hardness: number,
    ) {
      useCleanupQueue.setState({ drawing: false })
      const preferences = query.data ?? (await call(commands.getCleanupSettings))
      if (useCleanupQueue.getState().project !== project) return null
      if (preferences.remove_mode === 'queue') {
        addCleanupArea(page, {
          kind: 'stroke',
          stroke: { points: points.map((point) => ({ ...point })), diameter, hardness },
        })
        return null
      }
      return call(commands.commitInpaint, revision, points, diameter, hardness)
    },
  }
}
