'use client'

import { AlertTriangle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useProject } from '@/lib/queries'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@koharu/ui/components/dialog'

import { useReadyCheck, useReadyCheckDialog } from './data'
import { ReadyCheckPanel } from './ReadyCheckPanel'

export function ReadyCheckDialog() {
  const { t } = useTranslation()
  const { isOpen, close } = useReadyCheckDialog()
  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && close()}>
      <DialogContent showCloseButton={false} className='max-h-[85vh] overflow-y-auto sm:max-w-2xl'>
        <DialogHeader>
          <DialogTitle>{t('readyCheck.title')}</DialogTitle>
          <DialogDescription>{t('readyCheck.description')}</DialogDescription>
        </DialogHeader>
        {isOpen && <ReadyCheckPanel showDescription={false} />}
        <Button variant='outline' onClick={close}>
          {t('chapterContext.close')}
        </Button>
      </DialogContent>
    </Dialog>
  )
}

export function ReadyCheckIndicator() {
  const { t } = useTranslation()
  const project = useProject().data
  const check = useReadyCheck()
  const open = useReadyCheckDialog((state) => state.open)
  if (!project || (!check.isError && !check.data?.some((item) => item.status !== 'ready')))
    return null
  return (
    <Button
      size='sm'
      variant='ghost'
      className='h-6 gap-1 px-2 text-[11px] text-amber-600 dark:text-amber-400'
      onClick={open}
      title={t(check.isError ? 'readyCheck.loadError' : 'readyCheck.attention')}
    >
      <AlertTriangle className='size-3.5' />
      {t('readyCheck.title')}
    </Button>
  )
}
