'use client'

import { useEffect, type RefObject } from 'react'
import { create } from 'zustand'

import { projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import type { Bounds, ProjectInfo } from '@koharu/bridge/protocol'

// Wait for the selected page's prepared frame before changing its camera.
export const useCleanupFindingFocus = create<{
  request: { project: string; page: string; bounds: Bounds } | null
}>()(() => ({ request: null }))

export function useFocusCleanupFinding(
  surface: RefObject<HTMLDivElement | null>,
  project: string | undefined,
  page: string | undefined,
  ready: boolean,
) {
  const request = useCleanupFindingFocus((state) => state.request)
  useEffect(() => {
    if (!request) return
    const active = queryClient.getQueryData<ProjectInfo | null>(projectKey)
    if (active?.name !== request.project || active.active_page !== request.page) {
      useCleanupFindingFocus.setState({ request: null })
      return
    }
    // Query observers may render after the transient focus request is published.
    if (request.project !== project || request.page !== page) return
    if (!ready || !surface.current) return
    const viewport = surface.current.getBoundingClientRect()
    if (viewport.width <= 0 || viewport.height <= 0) return
    const dpr = window.devicePixelRatio || 1
    const { x, y, width, height } = request.bounds
    const zoom = Math.min(
      4 * dpr,
      (viewport.width * dpr) / (width + 96),
      (viewport.height * dpr) / (height + 96),
    )
    useKoharuStore.setState({
      camera: {
        zoom,
        translation: [
          (viewport.width * dpr) / 2 - (x + width / 2) * zoom,
          (viewport.height * dpr) / 2 - (y + height / 2) * zoom,
        ],
        fitted: false,
      },
    })
    useCleanupFindingFocus.setState({ request: null })
  }, [page, project, ready, request, surface])
}
