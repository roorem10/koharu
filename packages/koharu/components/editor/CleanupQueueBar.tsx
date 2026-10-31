'use client'

import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'

import { AreaOverlay } from '@/components/cleanup/AreaOverlay'
import { cleanQueuedAreas, clearCleanupAreas, useCleanupQueue } from '@/components/cleanup/queue'
import { originalVisible } from '@/components/qc/compare'
import { usePage } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { Button } from '@koharu/ui/components/button'

const emptyAreas: never[] = []

export function CleanupQueueBar() {
  const { t } = useTranslation()
  const page = usePage().data
  const areas = useCleanupQueue((state) =>
    page ? (state.pages[page.id] ?? emptyAreas) : emptyAreas,
  )
  const pending = useCleanupQueue((state) => (page ? !!state.pending[page.id] : false))
  const submitting = useCleanupQueue((state) =>
    page ? state.pending[page.id]?.job === null : false,
  )
  const drawing = useCleanupQueue((state) => state.drawing)
  const jobs = useKoharuStore((state) => state.jobs)
  const busy = Object.values(jobs).some((job) => job.state === 'running')
  useEffect(() => {
    const down = (event: KeyboardEvent) => {
      if (
        !page ||
        event.repeat ||
        event.ctrlKey ||
        event.metaKey ||
        event.altKey ||
        originalVisible()
      )
        return
      const state = useKoharuStore.getState()
      if (state.tool !== 'remove' || state.settingsOpen) return
      if (
        event.target instanceof Element &&
        event.target.closest(
          'input, textarea, select, button, [contenteditable="true"], [role="dialog"], [role="slider"]',
        )
      )
        return
      if (event.key === 'Enter') {
        if (!useCleanupQueue.getState().pages[page.id]?.length) return
        event.preventDefault()
        event.stopImmediatePropagation()
        void cleanQueuedAreas(page.id).catch(() => undefined)
      } else if (event.key === 'Escape') {
        clearCleanupAreas(page.id)
        useCleanupQueue.setState({ drawing: false })
        // Let the canvas cancel any gesture still held by the pointer.
      }
    }
    window.addEventListener('keydown', down, true)
    return () => window.removeEventListener('keydown', down, true)
  }, [page])
  if (!page || !areas.length) return null
  return (
    <>
      <AreaOverlay areas={areas} />
      <div
        data-canvas-control
        className='absolute bottom-4 left-1/2 z-30 flex max-w-[calc(100%-2rem)] -translate-x-1/2 items-center gap-3 rounded-xl border border-border bg-[var(--surface-floating)] px-3 py-2 shadow-lg'
        onPointerDown={(event) => event.stopPropagation()}
      >
        <span role='status' className='text-xs whitespace-nowrap'>
          {t('cleanup.queued', { count: areas.length })}
        </span>
        <Button
          type='button'
          size='sm'
          disabled={pending || busy || drawing}
          onClick={() => void cleanQueuedAreas(page.id).catch(() => undefined)}
        >
          {t('cleanup.clean')}
        </Button>
        <Button
          type='button'
          size='sm'
          variant='ghost'
          disabled={submitting}
          onClick={() => clearCleanupAreas(page.id)}
        >
          {t('cleanup.clear')}
        </Button>
      </div>
    </>
  )
}
