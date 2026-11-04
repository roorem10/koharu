'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { usePages, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@koharu/ui/components/dialog'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

import { type PrepOptions, usePreparePages, useRawPrepBusy, useRawPrepSettings } from './data'
import { PrepOptionFields } from './PrepOptionFields'
import { PrepPreview } from './PrepPreview'
import { PrepResultView } from './PrepResult'

export function RawPrepDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation()
  const settings = useRawPrepSettings()
  const project = useProject().data
  const pages = usePages().data ?? []
  const selected = useKoharuStore((state) => state.selectedPages)
  const [scope, setScope] = useState<'selected' | 'all'>(selected.length ? 'selected' : 'all')
  const [draft, setDraft] = useState<PrepOptions | null>(null)
  const options = draft ?? settings.data?.options
  const prepare = usePreparePages()
  const busy = useRawPrepBusy()
  const targets = pages.filter((page) => scope === 'all' || selected.includes(page.id))
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !prepare.isPending) onClose()
      }}
    >
      <DialogContent className='max-h-[90vh] overflow-auto sm:max-w-3xl' showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{t('rawPrep.fixLevels')}</DialogTitle>
          <DialogDescription>{t('rawPrep.description')}</DialogDescription>
        </DialogHeader>
        {prepare.data ? (
          <PrepResultView result={prepare.data} />
        ) : !options ? (
          <p role='status'>{t(settings.isError ? 'rawPrep.loadError' : 'rawPrep.loading')}</p>
        ) : (
          <>
            <PrepOptionFields value={options} onChange={setDraft} disabled={prepare.isPending} />
            <Select
              value={scope}
              onValueChange={(scope) => scope && setScope(scope as 'selected' | 'all')}
              disabled={prepare.isPending}
            >
              <SelectTrigger aria-label={t('rawPrep.scope')} className='w-full'>
                <SelectValue>{t(`rawPrep.${scope}`)}</SelectValue>
              </SelectTrigger>
              <SelectContent>
                <SelectItem
                  value='selected'
                  disabled={!selected.some((id) => pages.some((page) => page.id === id))}
                >
                  {t('rawPrep.selected')}
                </SelectItem>
                <SelectItem value='all'>{t('rawPrep.all')}</SelectItem>
              </SelectContent>
            </Select>
            <p className='text-xs text-muted-foreground'>{t('rawPrep.cleanupHelp')}</p>
            {project && (
              <PrepPreview
                page={targets[0]?.id}
                label={targets[0]?.label ?? ''}
                options={options}
                project={project.name}
                revision={project.revision}
              />
            )}
          </>
        )}
        <DialogFooter>
          <Button variant='outline' disabled={prepare.isPending} onClick={onClose}>
            {t(prepare.data ? 'rawPrep.done' : 'rawPrep.cancel')}
          </Button>
          {!prepare.data && (
            <Button
              disabled={
                !options ||
                !project ||
                targets.length === 0 ||
                busy ||
                (!options.levels && !options.whiten)
              }
              onClick={() => {
                if (options) prepare.mutate({ pages: targets.map((page) => page.id), options })
              }}
            >
              {t(prepare.isPending ? 'rawPrep.preparing' : 'rawPrep.apply')}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
