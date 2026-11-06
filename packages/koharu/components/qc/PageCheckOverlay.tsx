'use client'

import { useEffect, useRef } from 'react'

import { useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'

import { usePageCheckFocus } from './page-check-data'

export function PageCheckOverlay({ page }: { page: string }) {
  const root = useRef<HTMLDivElement>(null)
  const focus = usePageCheckFocus((state) => state.focus)
  const project = useProject().data
  const camera = useKoharuStore((state) => state.camera)
  const current =
    focus?.page === page && focus.project === project?.name && focus.revision === project.revision
      ? focus
      : null
  useEffect(() => {
    if (!current || !root.current) return
    const bounds = root.current.getBoundingClientRect()
    if (!bounds.width || !bounds.height) return
    const dpr = window.devicePixelRatio
    const area = current.area
    const zoom = Math.min(
      4 * dpr,
      Math.max(
        0.02,
        Math.min(
          (bounds.width * dpr) / (area.width + 120),
          (bounds.height * dpr) / (area.height + 120),
        ),
      ),
    )
    useKoharuStore.setState({
      camera: {
        zoom,
        translation: [
          (bounds.width * dpr) / 2 - (area.x + area.width / 2) * zoom,
          (bounds.height * dpr) / 2 - (area.y + area.height / 2) * zoom,
        ],
        fitted: false,
      },
    })
  }, [current])
  const dpr = window.devicePixelRatio
  return (
    <div
      ref={root}
      className='pointer-events-none absolute inset-0 z-10 overflow-hidden'
      aria-hidden
    >
      {current && (
        <div
          data-testid='page-check-area'
          className='absolute border-2 border-amber-400 shadow-[0_0_0_1px_black]'
          style={{
            left: (current.area.x * camera.zoom + camera.translation[0]) / dpr,
            top: (current.area.y * camera.zoom + camera.translation[1]) / dpr,
            width: (current.area.width * camera.zoom) / dpr,
            height: (current.area.height * camera.zoom) / dpr,
          }}
        />
      )}
    </div>
  )
}
