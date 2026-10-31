'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { QcFinding } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

import { markFinding, navigateFinding } from './data'

export function FindingRow({
  finding,
  hidden = false,
  navigate = true,
}: {
  finding: QcFinding
  hidden?: boolean
  navigate?: boolean
}) {
  const { t } = useTranslation()
  const [busy, setBusy] = useState(false)
  const label =
    finding.flag?.kind === 'review'
      ? finding.detail
      : finding.flag?.kind === 'glossary'
        ? t('qc.glossaryFlag', { term: finding.detail })
        : t(`qc.checks.${finding.check}`, { detail: finding.detail || t('qc.defaultFont') })
  const run = async (action: () => Promise<void>) => {
    setBusy(true)
    try {
      await action()
    } catch {
      /* call reports the error. */
    } finally {
      setBusy(false)
    }
  }
  return (
    <div
      className={`flex min-w-0 items-center gap-1 border-b border-border/40 px-2 py-1.5 ${hidden ? 'opacity-60' : ''}`}
    >
      <button
        type='button'
        disabled={busy || !navigate}
        onClick={() => void run(() => navigateFinding(finding))}
        className='min-w-0 flex-1 rounded-md p-1 text-left hover:bg-foreground/5 disabled:pointer-events-none'
      >
        <span
          className={`block text-[9px] ${finding.flag?.kind === 'review' ? 'text-blue-500' : 'text-muted-foreground'}`}
        >
          {t(finding.flag?.kind === 'review' ? 'qc.agent' : 'qc.autoChecks')}
        </span>
        <span className='block text-[11px] break-words'>{label}</span>
        <span
          className='block truncate text-[10px] text-muted-foreground'
          title={finding.translation}
        >
          {finding.translation || t('qc.noTranslation')}
        </span>
      </button>
      <Button
        size='sm'
        variant='ghost'
        disabled={busy}
        className='h-6 shrink-0 px-1.5 text-[10px]'
        aria-label={t(hidden ? 'qc.undoFor' : 'qc.okFor', { check: label })}
        onClick={() => void run(() => markFinding(finding, hidden))}
      >
        {t(hidden ? 'qc.undoOk' : 'qc.ok')}
      </Button>
    </div>
  )
}
