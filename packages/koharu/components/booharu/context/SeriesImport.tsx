'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { importSeries, useSeriesList, type FullSeries } from '@/lib/booharu/context'
import type { SeriesImport } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from '@koharu/ui/components/dialog'

/** Import series' preview, Booharu 0.0.4's import choice: the name of the series in the file the
 * user picked, `found`, and how many terms and characters it holds, imported with Import; when a
 * series of its id is saved already, Replace saves it in that one's place, and Import as separate
 * copy keeps both, the file's with a new id. `onImported` gets the series as saved; a failed import
 * shows its error, as every failed command's does, and keeps the preview open. */
export function SeriesImportDialog({
  found,
  onImported,
  onClose,
}: {
  found: SeriesImport
  onImported: (series: FullSeries) => void
  onClose: () => void
}) {
  const { t } = useTranslation()
  const saved = useSeriesList().data?.find((series) => series.id === found.series.id)
  const [busy, setBusy] = useState(false)
  const { series } = found

  const save = (replace: boolean) => {
    setBusy(true)
    importSeries(series, replace)
      .then(onImported)
      .catch(() => undefined)
      .finally(() => setBusy(false))
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent showCloseButton={false} className='gap-3 sm:max-w-md'>
        <DialogTitle className='text-[13px] font-semibold'>
          {t('booharu.context.importSeries')}
        </DialogTitle>
        <DialogDescription className='grid gap-0.5 text-[11px]'>
          <span className='text-[12px] font-medium break-words text-foreground'>{series.name}</span>
          <span>
            {t('booharu.context.termCount', { count: series.glossary?.length ?? 0 })}
            {' · '}
            {t('booharu.context.characterCount', { count: series.characters?.length ?? 0 })}
          </span>
        </DialogDescription>
        {found.exists && (
          <p className='text-[11px] break-words'>
            {t('booharu.context.importExists', { name: saved?.name ?? series.name })}
          </p>
        )}
        <DialogFooter>
          <Button type='button' variant='ghost' disabled={busy} onClick={onClose}>
            {t('common.cancel')}
          </Button>
          {found.exists ? (
            <>
              <Button
                type='button'
                variant='outline'
                disabled={busy}
                aria-busy={busy}
                onClick={() => save(false)}
              >
                {t('booharu.context.keepBoth')}
              </Button>
              <Button
                type='button'
                variant='destructive'
                disabled={busy}
                aria-busy={busy}
                onClick={() => save(true)}
              >
                {t('booharu.context.replace')}
              </Button>
            </>
          ) : (
            <Button type='button' disabled={busy} aria-busy={busy} onClick={() => save(false)}>
              {t('booharu.context.import')}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
