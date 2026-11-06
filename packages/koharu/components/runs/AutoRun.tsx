'use client'

import { useQuery } from '@tanstack/react-query'
import { Sparkles } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type AutoRunPreview,
  type AutoRunSummary,
  type Scope,
} from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@koharu/ui/components/dialog'

import { useRunSettings } from './data'
import { prepareFinishSound } from './sound'

export function AutoRunButton({ disabled }: { disabled: boolean }) {
  const { t } = useTranslation()
  const settings = useRunSettings().data
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button
        size='sm'
        variant='outline'
        className='h-auto min-h-7 px-2.5 text-[11px] whitespace-normal'
        disabled={disabled || !settings}
        onClick={() => setOpen(true)}
      >
        <Sparkles className='size-3 shrink-0' />
        {t('autoRun.button', {
          quality: t(`autoRun.qualities.${settings?.auto_run_quality ?? 'balanced'}`),
        })}
      </Button>
      <AutoRunDialog open={open} onOpenChange={setOpen} scope={{ scope: 'project' }} />
    </>
  )
}

export function AutoRunDialog({
  open,
  onOpenChange,
  scope,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  scope: Scope
}) {
  const { t } = useTranslation()
  const settings = useRunSettings().data
  const project = useProject().data
  const running = useKoharuStore((state) =>
    Object.values(state.jobs).some((job) => job.state === 'running'),
  )
  const [busy, setBusy] = useState(false)
  const quality = settings?.auto_run_quality ?? 'balanced'
  const preview = useQuery({
    queryKey: ['auto-run-preview', project?.name, project?.revision, scope, settings],
    queryFn: () => call(commands.previewAutoRun, scope, quality),
    enabled: open && !!project && !!settings,
    staleTime: 0,
  })
  const start = async () => {
    if (!preview.data || running || busy) return
    prepareFinishSound()
    setBusy(true)
    try {
      await call(commands.autoRun, scope, preview.data.quality)
      onOpenChange(false)
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog open={open} onOpenChange={(next) => !busy && onOpenChange(next)}>
      <DialogContent className='sm:max-w-lg' showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>
            {t('autoRun.button', { quality: t(`autoRun.qualities.${quality}`) })}
          </DialogTitle>
          <DialogDescription>{t('autoRun.confirm')}</DialogDescription>
        </DialogHeader>
        {preview.data ? (
          <AutoRunPreviewContent preview={preview.data} />
        ) : (
          <p role='status'>{t(preview.isError ? 'runs.loadError' : 'runs.loading')}</p>
        )}
        <DialogFooter>
          <Button variant='outline' disabled={busy} onClick={() => onOpenChange(false)}>
            {t('runs.cancel')}
          </Button>
          <Button
            disabled={busy || running || !preview.data || preview.isFetching || preview.isError}
            onClick={() => void start().catch(() => undefined)}
          >
            {t('autoRun.start')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export function AutoRunPreviewContent({ preview }: { preview: AutoRunPreview }) {
  const { t } = useTranslation()
  return (
    <div className='grid min-w-0 gap-3 text-sm'>
      <p>{t(preview.can_read_images ? 'autoRun.visionYes' : 'autoRun.visionNo')}</p>
      <p>{t('autoRun.estimate', { pages: preview.pages, requests: preview.estimated_requests })}</p>
      <ol className='grid max-h-72 list-decimal gap-2 overflow-auto pl-5'>
        {preview.steps.map((step) => (
          <li key={step.id} className='break-words'>
            <span>{t(`autoRun.steps.${step.id}`)}</span>
            {!step.needed && step.reason && (
              <span className='text-muted-foreground'>
                {' '}
                — {t(`autoRun.reasons.${step.reason}`)}
              </span>
            )}
          </li>
        ))}
      </ol>
    </div>
  )
}

export function AutoRunSummaryContent({ summary }: { summary: AutoRunSummary }) {
  const { t } = useTranslation()
  return (
    <div className='grid gap-2 text-[11px]'>
      <p role='status'>
        {t('autoRun.summary', {
          pages: summary.pages,
          failures: summary.failures,
          seconds: Math.round(summary.elapsed_seconds),
        })}
      </p>
      <ul className='grid max-h-48 gap-1 overflow-auto'>
        {summary.steps.map((step) => (
          <li key={step.id} className='break-words'>
            {t(`autoRun.steps.${step.id}`)}: {t(`autoRun.status.${step.status}`)}
            {step.reason && <> — {t(`autoRun.reasons.${step.reason}`)}</>}
            {step.error && <p className='text-destructive'>{step.error}</p>}
          </li>
        ))}
      </ul>
    </div>
  )
}
