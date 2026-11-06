'use client'

import { useId } from 'react'

import { useQcResults } from '@/components/qc/data'
import { usePage } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'

export function LeftoverOverlay() {
  const page = usePage().data
  const report = useQcResults().data
  const camera = useKoharuStore((state) => state.camera)
  const clip = useId()
  const findings =
    report?.findings.filter(
      (finding) =>
        finding.page === page?.id && finding.check === 'leftover_cleanup' && finding.bounds,
    ) ?? []
  if (!page || !findings.length) return null
  const dpr = window.devicePixelRatio || 1
  return (
    <svg
      aria-hidden
      data-testid='cleanup-leftovers'
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
        <g
          clipPath={`url(#${clip})`}
          fill='none'
          stroke='#f59e0b'
          strokeWidth={2}
          strokeDasharray='6 3'
        >
          {findings.map((finding, index) => (
            <rect
              key={`${finding.layer}:${index}`}
              {...finding.bounds!}
              vectorEffect='non-scaling-stroke'
            />
          ))}
        </g>
      </g>
    </svg>
  )
}
