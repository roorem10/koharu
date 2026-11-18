'use client'

import { RotateCcw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { retryFailedPages, useRetryable } from '@/lib/booharu/jobs'
import { useKoharuStore } from '@/lib/store'
import { Button } from '@koharu/ui/components/button'

/** Retry failed pages in the Run menu's root view, while the last run in the open project has
 * failed pages to queue again, as its list in the Activity center does; it waits while a run goes.
 * `onRetry` closes the menu as the click queues them. */
export function RetryFailedRow({ onRetry }: { onRetry: () => void }) {
  const { t } = useTranslation()
  const job = useRetryable()
  const running = useKoharuStore((state) =>
    Object.values(state.jobs).some((other) => other.state === 'running'),
  )
  if (!job) return null

  return (
    <Button
      type='button'
      variant='ghost'
      size='sm'
      disabled={running}
      className='h-auto min-h-8 min-w-0 justify-start gap-2 rounded-lg px-2 py-1.5 text-start text-[11px] font-normal whitespace-normal hover:bg-primary/10'
      onClick={() => {
        onRetry()
        void retryFailedPages(job).catch(() => undefined)
      }}
    >
      <RotateCcw className='size-3.5 text-primary' />
      <span className='min-w-0'>{t('booharu.runs.retryFailedPages')}</span>
    </Button>
  )
}
