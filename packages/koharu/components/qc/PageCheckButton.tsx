'use client'

import { ScanSearch } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { projectKey, queryClient, usePage, usePages, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type ProjectInfo } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@koharu/ui/components/dialog'

import { pageCheckKey } from './page-check-data'

export function PageCheckButton() {
  const { t } = useTranslation()
  const project = useProject().data
  const page = usePage().data
  const pages = usePages(false).data ?? []
  const running = useKoharuStore((state) =>
    Object.values(state.jobs).some((job) => job.state === 'running'),
  )
  const [openedFor, setOpenedFor] = useState<string | null>(null)
  const [all, setAll] = useState(false)
  const [busy, setBusy] = useState(false)
  const start = async () => {
    if (!page || !project || project.name !== openedFor) return
    setBusy(true)
    try {
      if (queryClient.getQueryData<ProjectInfo | null>(projectKey)?.name !== openedFor) return
      await call(commands.startPageCheck, all ? [] : [page.id])
      setOpenedFor(null)
      await queryClient.invalidateQueries({ queryKey: pageCheckKey })
    } catch {
      // call reports the error; keep the confirmation available for retry.
    } finally {
      setBusy(false)
    }
  }
  return (
    <>
      <Button
        size='sm'
        variant='ghost'
        className='h-7 text-[10px]'
        disabled={!page || running || busy}
        onClick={() => {
          setAll(false)
          setOpenedFor(project?.name ?? null)
        }}
      >
        <ScanSearch className='size-3.5' /> {t('pageCheck.checkPage')}
      </Button>
      <Dialog
        open={openedFor !== null && openedFor === project?.name}
        onOpenChange={(open) => {
          if (!open && !busy) setOpenedFor(null)
        }}
      >
        <DialogContent showCloseButton={false} className='sm:max-w-md'>
          <DialogHeader>
            <DialogTitle>{t('pageCheck.title')}</DialogTitle>
            <DialogDescription>
              {t('pageCheck.confirm', { count: all ? pages.length : 1 })}
            </DialogDescription>
          </DialogHeader>
          <div className='flex flex-wrap gap-2' role='group' aria-label={t('pageCheck.scope')}>
            <Button
              size='sm'
              variant={all ? 'outline' : 'secondary'}
              aria-pressed={!all}
              disabled={busy}
              onClick={() => setAll(false)}
            >
              {t('pageCheck.currentPage')}
            </Button>
            <Button
              size='sm'
              variant={all ? 'secondary' : 'outline'}
              aria-pressed={all}
              disabled={busy || !pages.length}
              onClick={() => setAll(true)}
            >
              {t('pageCheck.allPages')}
            </Button>
          </div>
          <p className='text-xs text-muted-foreground'>{t('pageCheck.optional')}</p>
          <DialogFooter>
            <Button size='sm' variant='outline' disabled={busy} onClick={() => setOpenedFor(null)}>
              {t('common.cancel')}
            </Button>
            <Button size='sm' disabled={busy || running} onClick={() => void start()}>
              {t('pageCheck.start')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
