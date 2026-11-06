'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { pageKey, pagesKey, projectKey, refresh } from '@/lib/queries'
import { commands, type EntityId } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from '@koharu/ui/components/dialog'
import { NativeSelect, NativeSelectOption } from '@koharu/ui/components/native-select'
import { toast } from '@koharu/ui/components/toast'

export function SizeConsistencyDialog({
  open,
  onOpenChange,
  page,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  page: EntityId | null
}) {
  const { t } = useTranslation()
  const [scope, setScope] = useState<'page' | 'chapter'>('page')
  const [busy, setBusy] = useState(false)
  const apply = async () => {
    if (busy) return
    setBusy(true)
    try {
      const result = await call(
        commands.makeSizesConsistent,
        scope === 'page' && page ? [page] : null,
      )
      await refresh(projectKey, pagesKey, pageKey)
      toast.add({
        type: 'success',
        title: t('textStyles.applied', { count: result.changed }),
        description: [
          result.median_fraction === null
            ? t('sizeConsistency.noDialogue')
            : t('sizeConsistency.reason', {
                median: Math.round(result.median_fraction * 10000) / 100,
              }),
          result.kept_manual ? t('textStyles.keptManual', { count: result.kept_manual }) : '',
          result.skipped ? t('sizeConsistency.skipped', { count: result.skipped }) : '',
        ]
          .filter(Boolean)
          .join(' '),
      })
      onOpenChange(false)
    } catch {
      // The shared command handler reports failures; leave the scope available to retry.
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog
      open={open}
      onOpenChange={(value) => {
        if (!busy) onOpenChange(value)
      }}
    >
      <DialogContent showCloseButton={!busy}>
        <DialogTitle>{t('sizeConsistency.action')}</DialogTitle>
        <DialogDescription>{t('sizeConsistency.checkHelp')}</DialogDescription>
        <label className='grid gap-2 text-sm'>
          {t('sizeConsistency.scope')}
          <NativeSelect
            aria-label={t('sizeConsistency.scope')}
            value={page ? scope : 'chapter'}
            disabled={busy}
            className='w-full'
            onChange={(event) => setScope(event.target.value as 'page' | 'chapter')}
          >
            <NativeSelectOption value='page' disabled={!page}>
              {t('sizeConsistency.page')}
            </NativeSelectOption>
            <NativeSelectOption value='chapter'>
              {t('sizeConsistency.chapter')}
            </NativeSelectOption>
          </NativeSelect>
        </label>
        <DialogFooter>
          <Button variant='outline' disabled={busy} onClick={() => onOpenChange(false)}>
            {t('textStyles.cancel')}
          </Button>
          <Button disabled={busy} onClick={() => void apply()}>
            {t('sizeConsistency.action')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
