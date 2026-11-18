'use client'

import { getVersion } from '@tauri-apps/api/app'
import Image from 'next/image'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@koharu/ui/components/dialog'

const author = 'roorem10' // Booharu: the owner's GitHub handle, as the fork rules write it.
// "Booharu 0.0.5" shows as 0.0.5; the build commit is shown on hover, for bug reports.
const forkVersion = process.env.NEXT_PUBLIC_KOHARU_FORK_VERSION?.replace(/^Booharu\s+/, '')
const forkCommit = process.env.NEXT_PUBLIC_KOHARU_FORK_COMMIT

export function AboutDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const [version, setVersion] = useState<string | null>(null)

  useEffect(() => {
    if (!open || version) return
    let active = true
    void getVersion()
      .then((current) => {
        if (active) setVersion(current)
      })
      .catch(() => undefined)
    return () => {
      active = false
    }
  }, [open, version])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-w-xs gap-4 p-5'>
        <div className='flex items-center gap-3'>
          <div className='grid size-12 shrink-0 place-items-center rounded-xl bg-primary/10'>
            <Image src='/icon.png' alt='' width={30} height={30} priority />
          </div>
          <DialogHeader className='min-w-0 gap-1'>
            <DialogTitle className='text-[15px]'>Booharu</DialogTitle>
            <DialogDescription className='text-[11px]'>{t('about.description')}</DialogDescription>
          </DialogHeader>
        </div>

        <div className='grid gap-2 border-t border-border/70 pt-3 text-[11px]'>
          {/* The version Tauri reports is the Koharu release Booharu is built on. */}
          <p className='text-muted-foreground'>
            {t('booharu.about.basedOn', { name: version ? `Koharu ${version}` : 'Koharu' })}
          </p>
          <dl className='grid grid-cols-[auto_1fr] gap-x-4 gap-y-2'>
            <dt className='text-muted-foreground'>{t('about.author')}</dt>
            <dd className='text-right font-medium'>{author}</dd>
            <dt className='text-muted-foreground'>{t('about.version')}</dt>
            <dd className='text-right font-medium' title={forkCommit}>
              {forkVersion || t('about.unavailable')}
            </dd>
          </dl>
        </div>
      </DialogContent>
    </Dialog>
  )
}
