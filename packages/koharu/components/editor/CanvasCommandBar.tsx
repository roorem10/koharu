'use client'

import { ScanEye } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { InferenceControl } from '@/components/editor/InferenceControl'
import { useCompareStore } from '@/components/qc/compare'
import { call } from '@/lib/backend'
import { usePage } from '@/lib/queries'
import { pipelineStages, useKoharuStore, type PipelineScope } from '@/lib/store'
import { commands, type Scope, type Stage } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

export function CanvasCommandBar() {
  const { t } = useTranslation()
  const pinned = useCompareStore((state) => state.pinned)
  const toggle = useCompareStore((state) => state.toggle)
  const page = usePage().data
  const selectedPages = useKoharuStore((state) => state.selectedPages)
  const jobs = useKoharuStore((state) => state.jobs)
  const running = Object.values(jobs).find((job) => job.state === 'running')

  const run = (selection: PipelineScope, stages: Stage[]) => {
    if (!page) return
    const scope: Scope =
      selection === 'project'
        ? { scope: 'project' }
        : selection === 'selected-pages'
          ? { scope: 'pages', value: selectedPages }
          : { scope: 'pages', value: [page.id] }
    const operation =
      stages.length === pipelineStages.length
        ? ({ operation: 'full' } as const)
        : stages.length === 1
          ? ({ operation: 'only', stage: stages[0]! } as const)
          : ({ operation: 'stages', stages } as const)
    void call(commands.process, scope, operation).catch(() => undefined)
  }

  return (
    <header className='flex h-10 shrink-0 items-center gap-2 border-b border-border/80 bg-[var(--surface-toolbar)] px-2.5'>
      <div className='min-w-0 flex-1' />
      <Button
        size='sm'
        variant={pinned ? 'secondary' : 'ghost'}
        className='h-7 text-[10px]'
        aria-pressed={pinned}
        disabled={!page}
        onClick={toggle}
      >
        <ScanEye className='size-3.5' /> {t('qc.showOriginal')}
      </Button>
      <InferenceControl disabled={!page || Boolean(running)} onRun={run} />
    </header>
  )
}
