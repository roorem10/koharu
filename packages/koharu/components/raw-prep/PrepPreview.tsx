'use client'

import { useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { commands, type EntityId, type PrepPreview as Preview } from '@koharu/bridge/protocol'

import type { PrepOptions } from './data'

export function PrepPreview({
  page,
  label,
  options,
  project,
  revision,
}: {
  page: EntityId | undefined
  label: string
  options: PrepOptions
  project: string
  revision: number
}) {
  const { t } = useTranslation()
  const [settled, setSettled] = useState(options)
  useEffect(() => {
    const timeout = setTimeout(() => setSettled(options), 200)
    return () => clearTimeout(timeout)
  }, [options])
  const preview = useQuery({
    queryKey: ['raw-prep-preview', project, revision, page, settled],
    queryFn: () => (page ? call(commands.previewPrep, page, settled) : null),
    enabled: !!page,
    gcTime: 0,
  })
  const pending = settled !== options || preview.isFetching
  return (
    <section className='grid gap-2' aria-label={t('rawPrep.preview')} aria-busy={pending}>
      <p className='text-xs text-muted-foreground'>{t('rawPrep.previewPage', { page: label })}</p>
      {preview.isError ? (
        <p role='alert' className='text-xs text-destructive'>
          {t('rawPrep.previewError')}
        </p>
      ) : pending || !preview.data ? (
        <p role='status' className='py-8 text-center text-xs text-muted-foreground'>
          {t(page ? 'rawPrep.loadingPreview' : 'rawPrep.noPages')}
        </p>
      ) : (
        <PreviewImages preview={preview.data} />
      )}
      {options.levels && preview.data && !pending && (
        <p className='min-w-0 text-xs break-all text-muted-foreground'>
          {t('rawPrep.points', { black: preview.data.black, white: preview.data.white })}
        </p>
      )}
    </section>
  )
}

function PreviewImages({ preview }: { preview: Preview }) {
  const { t } = useTranslation()
  const [urls, setUrls] = useState<{ before: string; after: string } | null>(null)
  useEffect(() => {
    const create = (bytes: number[]) =>
      URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'image/png' }))
    const next = { before: create(preview.before), after: create(preview.after) }
    setUrls(next)
    return () => {
      URL.revokeObjectURL(next.before)
      URL.revokeObjectURL(next.after)
    }
  }, [preview])
  return (
    <div className='grid grid-cols-2 gap-3'>
      {(['before', 'after'] as const).map((side) => (
        <figure key={side} className='min-w-0 rounded-lg border bg-muted/30 p-2'>
          <figcaption className='mb-1 text-center text-xs font-medium'>
            {t(`rawPrep.${side}`)}
          </figcaption>
          {urls && (
            <img
              src={urls[side]}
              alt={t(`rawPrep.${side}`)}
              className='mx-auto h-64 max-h-[35vh] w-full object-contain'
            />
          )}
        </figure>
      ))}
    </div>
  )
}
