'use client'

import { useMutation, useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type EntityId, type QcFixScope } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

export const qcReviewKey = ['qc-review'] as const
export const qcFixSummaryKey = ['qc-fix-summary'] as const
export const qcFixHistoryKey = ['qc-fix-history'] as const

function useFixRevision() {
  const project = useProject().data
  const jobs = useKoharuStore((state) => state.jobs)
  const completed = Object.values(jobs)
    .filter((job) => job.state !== 'running')
    .map((job) => `${job.id}:${job.state}`)
    .join(',')
  return { project, completed }
}

export function useQcReview() {
  const { project, completed } = useFixRevision()
  return useQuery({
    queryKey: [...qcReviewKey, project?.name, project?.revision, completed],
    queryFn: () => call(commands.getQcReview),
    enabled: Boolean(project),
  })
}

export function useQcFixSummary() {
  const { project, completed } = useFixRevision()
  return useQuery({
    queryKey: [...qcFixSummaryKey, project?.name, project?.revision, completed],
    queryFn: () => call(commands.getQcFixSummary),
    enabled: Boolean(project),
  })
}

export function useQcFixHistory(layer: EntityId) {
  const { project, completed } = useFixRevision()
  return useQuery({
    queryKey: [...qcFixHistoryKey, project?.name, project?.revision, layer, completed],
    queryFn: () => call(commands.getQcFixHistory, layer),
    enabled: Boolean(project),
  })
}

export function FixQcButton({
  scope,
  disabled = false,
}: {
  scope: QcFixScope
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const running = useKoharuStore((state) =>
    Object.values(state.jobs).some((job) => job.state === 'running'),
  )
  const mutation = useMutation({ mutationFn: () => call(commands.fixQc, scope, false) })
  return (
    <Button
      size='sm'
      variant='outline'
      className='h-auto min-h-7 text-[11px] whitespace-normal'
      disabled={disabled || running || mutation.isPending}
      aria-label={t(scope.scope === 'boxes' ? 'qc.fixBox' : 'qc.fixAutomatically')}
      onClick={() => mutation.mutate()}
    >
      {t('qc.fixAutomatically')}
    </Button>
  )
}
