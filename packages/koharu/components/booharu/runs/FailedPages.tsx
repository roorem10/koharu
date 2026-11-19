'use client'

import { CircleAlert, X } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useReadingChapter } from '@/lib/booharu/booharuJobs'
import { retryFailedPages, retryRuns, showFailedPage } from '@/lib/booharu/jobs'
import { usePages } from '@/lib/queries'
import { useKoharuStore, type Job } from '@/lib/store'
import { Button } from '@koharu/ui/components/button'

/** A run's failed pages, in the Activity center in place of its row: for a run done with problems,
 * or a run over pages that failed. Each row has the page's number, which shows the page, the stage
 * that failed and why, cut to two lines; a page the project no longer has is left out, and with
 * none left, a failed run says why it failed. Retry failed pages queues them again, from the stage
 * each failed at, for a run over pages, while no run or read of the chapter goes; Dismiss takes the
 * list away. */
export function FailedPages({ job }: { job: Job }) {
  const { t } = useTranslation()
  const dismiss = useKoharuStore((state) => state.dismissJob)
  const reading = useReadingChapter()
  const running =
    useKoharuStore((state) =>
      Object.values(state.jobs).some((other) => other.state === 'running'),
    ) || reading
  const order = usePages().data?.map((page) => page.id) ?? []
  const [queuing, setQueuing] = useState(false)
  const rows = (job.booharu?.failures ?? []).flatMap((failure) => {
    const index = order.indexOf(failure.page)
    return index < 0 ? [] : [{ ...failure, number: index + 1 }]
  })
  const retryable = retryRuns(job, order).length > 0

  const retry = () => {
    setQueuing(true)
    void retryFailedPages(job)
      .catch(() => undefined)
      .finally(() => setQueuing(false))
  }

  return (
    <div className='grid grid-cols-[1rem_minmax(0,1fr)_2.25rem_1.5rem] items-start gap-x-2.5 gap-y-2 border-b p-3 text-[11px] last:border-b-0'>
      <CircleAlert className='mt-0.5 size-3.5 justify-self-center text-destructive' />
      <p className='col-start-2 col-end-4 min-w-0 text-destructive'>
        {job.state === 'failed'
          ? t('activity.processingFailed')
          : t('booharu.runs.finishedWithProblems')}
      </p>
      <Button
        size='icon-xs'
        variant='ghost'
        className='-mt-1'
        aria-label={t('activity.dismiss')}
        onClick={() => dismiss(String(job.id))}
      >
        <X />
      </Button>
      {!rows.length && job.error ? (
        <p className='col-start-2 col-end-5 min-w-0 text-[10px] break-words text-muted-foreground'>
          {job.error}
        </p>
      ) : null}
      {rows.length ? (
        <ul
          aria-label={t('booharu.runs.failedPages')}
          className='col-start-2 col-end-5 grid max-h-48 min-w-0 gap-2 overflow-y-auto'
        >
          {rows.map((row) => (
            <li key={row.page} className='min-w-0'>
              <div className='flex min-w-0 items-baseline gap-2'>
                <button
                  type='button'
                  className='shrink-0 rounded-sm font-medium text-primary outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring/25'
                  onClick={() => showFailedPage(row.page)}
                >
                  {t('navigator.pageNumber', { number: row.number })}
                </button>
                <span className='min-w-0 truncate text-muted-foreground'>
                  {t(`phase.${row.stage}`)}
                </span>
              </div>
              <p
                className='mt-0.5 line-clamp-2 text-[10px] break-words text-muted-foreground'
                title={row.reason}
              >
                {row.reason}
              </p>
            </li>
          ))}
        </ul>
      ) : null}
      {retryable ? (
        <Button
          type='button'
          size='sm'
          variant='outline'
          className='col-start-2 col-end-5 h-auto min-h-7 py-1 text-center text-[11px] whitespace-normal'
          disabled={running || queuing}
          onClick={retry}
        >
          {t('booharu.runs.retryFailedPages')}
        </Button>
      ) : null}
    </div>
  )
}

/** Retrying (2/3) under a running run's row while one of its pages waits for its next try: the
 * try the page that waited last waits for, of its tries. */
export function RetryingNote({ job }: { job: Job }) {
  const { t } = useTranslation()
  const retry = job.booharu?.retrying.at(-1)
  if (!retry) return null
  return (
    <p
      role='status'
      className='col-start-2 col-end-4 mt-0.5 truncate text-[10px] text-muted-foreground'
    >
      {t('booharu.runs.retrying', { attempt: retry.attempt, attempts: retry.attempts })}
    </p>
  )
}
