'use client'

import { Square } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { useBooharuJobs } from '@/lib/booharu/booharuJobs'
import { commands, type BooharuJob } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'

/** Booharu's jobs going, in the Activity center beside Koharu's runs: a read of the chapter, by
 * hand, by the agent, or as a Run press's step, as "Reading chapter", with the share of its
 * requests done and Stop, which stops it as the Run button's Stop does, through the pipeline's. */
export function BooharuJobRows() {
  const jobs = useBooharuJobs((state) => state.jobs)
  return Object.values(jobs).map((job) => <ReadRow key={job.id} job={job} />)
}

/** A read's row, laid out as a run's row is. A read of one request has no steps to count, so its
 * bar shows it going without a share done. */
function ReadRow({ job }: { job: BooharuJob }) {
  const { t } = useTranslation()
  const percent = job.steps > 1 ? Math.min(100, Math.round((job.step / job.steps) * 100)) : null
  return (
    <div className='border-b p-3 last:border-b-0'>
      <div className='grid grid-cols-[1rem_minmax(0,1fr)_2.25rem_1.5rem] items-start gap-x-2.5'>
        <span className='mt-1.5 size-1.5 justify-self-center rounded-full bg-primary' />
        <span className='block min-w-0 truncate text-[12px] font-medium'>
          {t('booharu.context.readingRow')}
        </span>
        <span className='pt-0.5 text-right text-[10px] tabular-nums'>
          {percent !== null ? `${percent}%` : null}
        </span>
        <Button
          size='icon-xs'
          variant='ghost'
          className='-mt-1'
          aria-label={t('activity.stop')}
          onClick={() => void call(commands.stopPipeline).catch(() => undefined)}
        >
          <Square className='size-2.5 fill-current' />
        </Button>
        <div className='col-start-2 col-end-4'>
          <div
            role='progressbar'
            aria-label={t('booharu.context.readingRow')}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={percent ?? undefined}
            className='mt-2 h-1 overflow-hidden rounded-full bg-muted'
          >
            <div
              className={`h-full rounded-full bg-primary ${percent === null ? 'w-1/2' : ''}`}
              style={percent === null ? undefined : { width: `${percent}%` }}
            />
          </div>
        </div>
      </div>
    </div>
  )
}
