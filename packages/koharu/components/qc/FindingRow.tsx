'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { QcFinding } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

import { markFinding, navigateFinding } from './data'
import { FixQcButton } from './fixes'

export function FindingRow({
  finding,
  hidden = false,
  navigate = true,
  review = false,
}: {
  finding: QcFinding
  hidden?: boolean
  navigate?: boolean
  review?: boolean
}) {
  const { t } = useTranslation()
  const [busy, setBusy] = useState(false)
  const label =
    finding.check === 'auto_fixed'
      ? t('qc.autoFixedReason', { reason: t(`qc.fixReasons.${finding.detail}`) })
      : finding.check === 'lettering_unsure'
        ? t('qc.letteringUnsure', { category: t(`lettering.categories.${finding.detail}`) })
        : finding.flag?.kind === 'review'
          ? finding.detail
          : finding.flag?.kind === 'glossary'
            ? t('qc.glossaryFlag', { term: finding.detail })
            : finding.check === 'too_small' && finding.detail.startsWith('word:')
              ? t('qc.smallWord', { detail: finding.detail.slice(5) })
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
      className={`flex min-w-0 flex-wrap items-center gap-1 border-b border-border/40 px-2 py-1.5 ${hidden ? 'opacity-60' : ''}`}
    >
      <button
        type='button'
        disabled={busy || !navigate}
        onClick={() => void run(() => navigateFinding(finding))}
        className='min-w-0 flex-1 basis-36 rounded-md p-1 text-left hover:bg-foreground/5 disabled:pointer-events-none'
      >
        {(!review || finding.flag?.kind === 'review') && (
          <span
            className={`block text-[9px] ${finding.flag?.kind === 'review' ? 'text-blue-500' : 'text-muted-foreground'}`}
          >
            {t(finding.flag?.kind === 'review' ? 'qc.agent' : 'qc.autoChecks')}
          </span>
        )}
        <span className='block text-[11px] break-words'>{label}</span>
        {finding.check !== 'leftover_cleanup' && (
          <span
            className='block truncate text-[10px] text-muted-foreground'
            title={finding.translation}
          >
            {finding.translation || t('qc.noTranslation')}
          </span>
        )}
      </button>
      {finding.check !== 'leftover_cleanup' && (
        <Button
          size='sm'
          variant='ghost'
          disabled={busy}
          className='h-6 shrink-0 px-1.5 text-[10px]'
          aria-label={t(hidden ? 'qc.undoFor' : review ? 'qc.looksGoodFor' : 'qc.okFor', {
            check: label,
          })}
          onClick={() => void run(() => markFinding(finding, hidden))}
        >
          {t(hidden ? 'qc.undoOk' : review ? 'qc.looksGood' : 'qc.ok')}
        </Button>
      )}
      {review &&
        !hidden &&
        finding.check !== 'auto_fixed' &&
        finding.check !== 'lettering_unsure' &&
        finding.check !== 'leftover_cleanup' &&
        finding.flag?.kind !== 'review' && (
          <div className='w-full px-1'>
            <FixQcButton scope={{ scope: 'boxes', layers: [finding.layer] }} />
          </div>
        )}
    </div>
  )
}
