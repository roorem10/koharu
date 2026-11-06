'use client'

import { useQuery } from '@tanstack/react-query'
import { ChevronDown, ScanSearch } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { prepareFinishSound } from '@/components/runs/sound'
import { call } from '@/lib/backend'
import { queryClient, usePage, usePages, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@koharu/ui/components/dropdown-menu'

export function CheckCleanupControl() {
  const { t } = useTranslation()
  const page = usePage().data
  const project = useProject().data
  const pages = usePages(false).data
  const jobs = useKoharuStore((state) => state.jobs)
  const completed = Object.values(jobs)
    .filter((job) => job.operation?.operation === 'check_cleanup' && job.state !== 'running')
    .map((job) => `${job.id}:${job.state}`)
    .join(',')
  const results = useQuery({
    queryKey: ['cleanup-check-results', project?.name, project?.revision],
    queryFn: () => call(commands.getCleanupCheckResults),
    enabled: Boolean(project),
  })
  useEffect(() => {
    if (completed) void queryClient.invalidateQueries({ queryKey: ['cleanup-check-results'] })
  }, [completed])
  const running = useKoharuStore((state) =>
    Object.values(state.jobs).some((job) => job.state === 'running'),
  )
  const pending = useRef(false)
  const [submitting, setSubmitting] = useState(false)
  const disabled = !page || running || submitting
  const run = async (pages: string[] | null) => {
    if (
      pending.current ||
      Object.values(useKoharuStore.getState().jobs).some((job) => job.state === 'running')
    )
      return
    pending.current = true
    setSubmitting(true)
    prepareFinishSound()
    try {
      await call(commands.checkCleanup, pages)
    } catch {
      // The shared command handler reports the error.
    } finally {
      pending.current = false
      setSubmitting(false)
    }
  }
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button size='sm' variant='ghost' className='h-7 text-[10px]' disabled={disabled} />
        }
      >
        <ScanSearch className='size-3.5' />
        {t('cleanup.check')}
        <ChevronDown className='size-3' />
      </DropdownMenuTrigger>
      <DropdownMenuContent align='end' className='w-72 max-w-[calc(100vw-2rem)]'>
        <DropdownMenuItem disabled={disabled} onClick={() => page && void run([page.id])}>
          {t('cleanup.checkPage')}
        </DropdownMenuItem>
        <DropdownMenuItem disabled={disabled} onClick={() => void run(null)}>
          {t('cleanup.checkAll')}
        </DropdownMenuItem>
        {!!results.data?.length && (
          <>
            <DropdownMenuSeparator />
            <div
              aria-label={t('cleanup.checkResults')}
              className='grid max-h-64 max-w-72 gap-2 overflow-y-auto px-2 py-1.5 text-[11px]'
            >
              {results.data.map((result) => (
                <div key={result.page} className='grid gap-1'>
                  <span className='truncate font-medium'>
                    {pages?.find((entry) => entry.id === result.page)?.label ??
                      (result.page === page?.id ? page.label : result.page)}
                  </span>
                  {result.retried && (
                    <p className='text-muted-foreground'>{t('cleanup.retryReason')}</p>
                  )}
                  {result.protected && result.findings.length > 0 && (
                    <p className='text-muted-foreground'>{t('cleanup.protectedReason')}</p>
                  )}
                  <p>
                    {t(
                      result.findings.length ? 'qc.checks.leftover_cleanup' : 'cleanup.clearResult',
                    )}
                  </p>
                </div>
              ))}
            </div>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
