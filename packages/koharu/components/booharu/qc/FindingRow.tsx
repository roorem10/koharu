'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { dismissFlag, markQcOk, qcLabel } from '@/lib/booharu/qc'
import type { Finding } from '@koharu/ipc/protocol'
import { Badge } from '@koharu/ui/components/badge'
import { Button } from '@koharu/ui/components/button'
import { cn } from '@koharu/ui/lib/utils'

/** One finding of the quality check, as Booharu 0.0.4 listed it: an Agent tag on the agent's flag,
 * what was found, and the line it's on, or No translation; then Dismiss on a flag, else Mark OK, or
 * Undo OK on one marked OK, which shows faded. Each is one undo step, after which the desktop
 * checks the page again. With `onShow`, pressing the row shows its box. */
export function FindingRow({ finding, onShow }: { finding: Finding; onShow?: () => void }) {
  const { t } = useTranslation()
  // The button pressed waits for its command, a press each.
  const [busy, setBusy] = useState(false)
  const label = qcLabel(t, finding)
  const flag = finding.source === 'agent'
  const settle = (command: Promise<unknown>) => {
    setBusy(true)
    void command.catch(() => undefined).finally(() => setBusy(false))
  }

  // The spaces between the parts keep them apart in the row's name. The line and the agent's words
  // keep their own direction, as English does in an Arabic interface, its full stop at its end.
  const body = (
    <>
      {flag && (
        <Badge variant='outline' className='mb-0.5 h-4 px-1.5 text-[9px]'>
          {t('booharu.qc.agent')}
        </Badge>
      )}{' '}
      <span className='block text-[11px] leading-4 break-words'>
        {flag ? <bdi>{label}</bdi> : label}
      </span>{' '}
      <span
        className='block truncate text-[10px] leading-4 text-muted-foreground'
        title={finding.text || undefined}
      >
        {finding.text ? <bdi>{finding.text}</bdi> : t('booharu.qc.noTranslation')}
      </span>
    </>
  )
  return (
    <div
      className={cn(
        'flex min-w-0 items-center gap-1 border-b border-border/40 px-2 py-1',
        finding.hidden && 'opacity-60',
      )}
    >
      {onShow ? (
        <button
          type='button'
          className='min-w-0 flex-1 rounded-md p-1 text-left hover:bg-foreground/5 focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:outline-none'
          onClick={onShow}
        >
          {body}
        </button>
      ) : (
        <div className='min-w-0 flex-1 p-1'>{body}</div>
      )}
      {flag ? (
        <Button
          type='button'
          size='xs'
          variant='ghost'
          disabled={busy}
          aria-label={t('booharu.qc.dismissFor', { check: label })}
          className='shrink-0 px-1.5 text-[10px]'
          onClick={() => settle(dismissFlag(finding))}
        >
          {t('booharu.qc.dismiss')}
        </Button>
      ) : (
        <Button
          type='button'
          size='xs'
          variant='ghost'
          disabled={busy}
          aria-label={t(finding.hidden ? 'booharu.qc.undoFor' : 'booharu.qc.okFor', {
            check: label,
          })}
          className='shrink-0 px-1.5 text-[10px]'
          onClick={() => settle(markQcOk(finding, !finding.hidden))}
        >
          {t(finding.hidden ? 'booharu.qc.undoOk' : 'booharu.qc.ok')}
        </Button>
      )}
    </div>
  )
}
