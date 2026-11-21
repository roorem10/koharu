'use client'

import { Square } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useBooharuJobs } from '@/lib/booharu/booharuJobs'
import { send } from '@/lib/queries'
import { commands, type BooharuJob } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { Progress, ProgressLabel, ProgressValue } from '@koharu/ui/components/progress'

/** Booharu's jobs going, in the Activity center beside Koharu's runs: a read of the chapter, by
 * hand, by the agent, or as a Run press's step, as "Reading chapter", with the share of its
 * requests done and Stop, which stops it as the Run button's Stop does, through the pipeline's. */
export function BooharuJobRows() {
  const jobs = useBooharuJobs((state) => state.jobs)
  return Object.values(jobs).map((job) => <ReadRow key={job.id} job={job} />)
}

/** A read's row, laid out as Koharu's rows of work under way are (ActivityCenter's ProgressRow):
 * its name and the share of its requests done, over its bar, with Stop beside them. A read of one
 * request has no steps to count, so its bar runs on its own. */
function ReadRow({ job }: { job: BooharuJob }) {
  const { t, i18n } = useTranslation()
  const counted = job.steps > 1
  return (
    <div className='grid grid-cols-[1rem_minmax(0,1fr)_1.5rem] items-start gap-x-2.5 border-b p-3 last:border-b-0'>
      <span className='mt-1.5 size-1.5 justify-self-center rounded-full bg-primary' />
      <Progress
        value={counted ? Math.min(job.step, job.steps) : null}
        max={counted ? job.steps : 100}
        locale={i18n.language}
        className='min-w-0 gap-x-2 gap-y-1.5'
      >
        <ProgressLabel className='min-w-0 flex-1 truncate text-[11px] font-medium'>
          {t('booharu.context.readingRow')}
        </ProgressLabel>
        <ProgressValue className='text-[10px] text-foreground' />
      </Progress>
      <Button
        size='icon-xs'
        variant='ghost'
        className='-mt-1'
        aria-label={t('activity.stop')}
        onClick={() => send(commands.stopPipeline)}
      >
        <Square className='size-2.5 fill-current' />
      </Button>
    </div>
  )
}
