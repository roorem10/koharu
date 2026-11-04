'use client'

import { useQuery } from '@tanstack/react-query'
import { LoaderCircle } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { useKoharuStore } from '@/lib/store'
import { commands, type Page } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

export function OriginalOverlay({ page }: { page: Page }) {
  const { t } = useTranslation()
  const camera = useKoharuStore((state) => state.camera)
  const [image, setImage] = useState<{ page: string; url: string; failed: boolean } | null>(null)
  const [dpr, setDpr] = useState(window.devicePixelRatio)
  useEffect(() => {
    const resize = () => setDpr(window.devicePixelRatio)
    window.addEventListener('resize', resize)
    window.visualViewport?.addEventListener('resize', resize)
    return () => {
      window.removeEventListener('resize', resize)
      window.visualViewport?.removeEventListener('resize', resize)
    }
  }, [])
  const source = useQuery({
    queryKey: ['qc-page-source', page.id],
    queryFn: async () => new Uint8Array(await call(commands.getPageSource, page.id)),
    gcTime: 0,
  })
  useEffect(() => {
    if (!source.data) return
    const url = URL.createObjectURL(new Blob([source.data.buffer]))
    setImage({ page: page.id, url, failed: false })
    return () => URL.revokeObjectURL(url)
  }, [page.id, source.data])
  const decodeFailed = image?.page === page.id && image.failed
  const url = image?.page === page.id && !decodeFailed ? image.url : null
  return (
    <div
      data-testid='original-view'
      className='pointer-events-none absolute inset-0 z-20 overflow-hidden bg-[var(--surface-canvas)]'
    >
      {url && (
        // eslint-disable-next-line @next/next/no-img-element
        <img
          src={url}
          alt={t('qc.original')}
          onError={() => setImage((image) => (image ? { ...image, failed: true } : image))}
          draggable={false}
          className='absolute top-0 left-0 max-w-none select-none'
          style={{
            width: page.size.width,
            height: page.size.height,
            transformOrigin: 'top left',
            transform: `translate(${camera.translation[0] / dpr}px, ${camera.translation[1] / dpr}px) scale(${camera.zoom / dpr})`,
          }}
        />
      )}
      {!url && !source.isError && !decodeFailed && (
        <div role='status' className='absolute inset-0 grid place-items-center'>
          <LoaderCircle className='size-5 animate-spin' aria-label={t('qc.loadingOriginal')} />
        </div>
      )}
      {(source.isError || decodeFailed) && (
        <div
          role='alert'
          className='pointer-events-auto absolute inset-0 flex flex-col items-center justify-center gap-2'
        >
          <p className='text-xs text-muted-foreground'>{t('qc.sourceFailed')}</p>
          <Button
            data-canvas-control
            size='sm'
            variant='outline'
            onClick={() => {
              setImage(null)
              void source.refetch()
            }}
          >
            {t('qc.retry')}
          </Button>
        </div>
      )}
      <span className='absolute top-3 right-3 rounded-md border border-border bg-popover/95 px-2 py-1 text-[10px] font-medium shadow-sm'>
        {t('qc.original')}
      </span>
    </div>
  )
}
