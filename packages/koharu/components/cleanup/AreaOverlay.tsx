'use client'

import { useId } from 'react'

import { usePage } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'

import type { CleanupArea } from './queue'

export function AreaOverlay({ areas }: { areas: CleanupArea[] }) {
  const page = usePage().data
  const camera = useKoharuStore((state) => state.camera)
  const clip = useId()
  if (!page || !areas.length) return null
  const dpr = window.devicePixelRatio || 1
  return (
    <svg
      aria-hidden
      data-testid='cleanup-areas'
      className='pointer-events-none absolute inset-0 z-10 size-full overflow-hidden'
    >
      <g
        transform={`translate(${camera.translation[0] / dpr} ${camera.translation[1] / dpr}) scale(${camera.zoom / dpr})`}
      >
        <defs>
          <clipPath id={clip}>
            <rect width={page.size.width} height={page.size.height} />
          </clipPath>
        </defs>
        <g clipPath={`url(#${clip})`} opacity={0.35} fill='#ef4444' stroke='#ef4444'>
          {areas.map((area) =>
            area.kind === 'rect' ? (
              <rect key={area.id} {...area.rect} stroke='none' />
            ) : area.stroke.points.length === 1 ? (
              <circle
                key={area.id}
                cx={area.stroke.points[0].x}
                cy={area.stroke.points[0].y}
                r={area.stroke.diameter / 2}
                stroke='none'
              />
            ) : (
              <path
                key={area.id}
                d={area.stroke.points.map((p, i) => `${i ? 'L' : 'M'} ${p.x} ${p.y}`).join(' ')}
                fill='none'
                strokeWidth={area.stroke.diameter}
                strokeLinecap='round'
                strokeLinejoin='round'
              />
            ),
          )}
        </g>
      </g>
    </svg>
  )
}
