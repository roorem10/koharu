'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { pageKey, pagesKey, projectKey, refresh } from '@/lib/queries'
import type { EntityId } from '@koharu/bridge/protocol'
import { commands } from '@koharu/bridge/protocol'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@koharu/ui/components/alert-dialog'
import { MenubarItem, MenubarSeparator } from '@koharu/ui/components/menubar'

import { useBoxesBusy } from './queries'

export function RedetectPageMenuItem({
  page,
  onRedetect,
}: {
  page: EntityId | null
  onRedetect: (page: EntityId) => void
}) {
  const { t } = useTranslation()
  const running = useBoxesBusy()
  return (
    <>
      <MenubarSeparator />
      <MenubarItem
        className='min-h-8 px-2 py-1 text-xs'
        disabled={!page || running}
        onClick={() => {
          if (page) onRedetect(page)
        }}
      >
        {t('boxes.redetectPage')}
      </MenubarItem>
    </>
  )
}

export function RedetectPageDialog({
  page,
  onClose,
}: {
  page: EntityId | null
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [busy, setBusy] = useState(false)
  const running = useBoxesBusy()
  const detect = async () => {
    if (!page || busy || running) return
    setBusy(true)
    try {
      await call(commands.redetectPage, page)
      await refresh(projectKey, pagesKey, pageKey, ['boxes'])
      onClose()
    } catch {
      /* call reports the error and the dialog remains available. */
    } finally {
      setBusy(false)
    }
  }
  return (
    <AlertDialog
      open={page !== null}
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t('boxes.redetectPage')}</AlertDialogTitle>
          <AlertDialogDescription>{t('boxes.redetectConfirmation')}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy}>{t('boxes.cancel')}</AlertDialogCancel>
          <AlertDialogAction
            disabled={busy || running}
            onClick={(event) => {
              event.preventDefault()
              void detect()
            }}
          >
            {t('boxes.redetectPage')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
