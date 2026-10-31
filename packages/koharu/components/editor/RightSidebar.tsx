'use client'

import { BookOpen, Bot, ListChecks, SlidersHorizontal } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { ContextPanel } from '@/components/context/ContextPanel'
import { discussLineEvent, openContextEvent } from '@/components/context/events'
import { AgentPanel } from '@/components/editor/AgentPanel'
import { Inspector } from '@/components/editor/Inspector'
import { useAutomaticQc, useQcResults } from '@/components/qc/data'
import { QcPanel } from '@/components/qc/QcPanel'
import { invalidateLineLabels } from '@/lib/line-labels'
import { useKoharuStore } from '@/lib/store'
import { Button } from '@koharu/ui/components/button'

export function RightSidebar() {
  const { t } = useTranslation()
  useAutomaticQc()
  const issueCount = useQcResults().data?.findings.length ?? 0
  const jobs = useKoharuStore((state) => state.jobs)
  const completedReads = Object.values(jobs)
    .filter((job) => job.state === 'finished')
    .map((job) => job.id)
    .join(',')
  useEffect(() => {
    void invalidateLineLabels()
  }, [completedReads])
  const [panel, setPanel] = useState<'agent' | 'properties' | 'context' | 'qc'>('properties')

  useEffect(() => {
    if (panel === 'qc' && issueCount === 0) setPanel('properties')
  }, [issueCount, panel])

  useEffect(() => {
    const open = () => setPanel('agent')
    const context = () => setPanel('context')
    window.addEventListener(openContextEvent, context)
    window.addEventListener(discussLineEvent, open)
    return () => {
      window.removeEventListener(discussLineEvent, open)
      window.removeEventListener(openContextEvent, context)
    }
  }, [])

  return (
    <aside className='flex h-full min-h-0 min-w-0 flex-col bg-[var(--surface-panel)]'>
      <div
        role='group'
        aria-label={t('qc.editorPanels')}
        className='flex h-10 shrink-0 items-center gap-1 border-b border-border/80 px-2.5'
      >
        <Button
          variant={panel === 'properties' ? 'secondary' : 'ghost'}
          aria-pressed={panel === 'properties'}
          size='sm'
          className='h-7 flex-1 text-[10px]'
          onClick={() => setPanel('properties')}
        >
          <SlidersHorizontal className='size-3' /> {t('agent.properties')}
        </Button>
        <Button
          variant={panel === 'context' ? 'secondary' : 'ghost'}
          aria-pressed={panel === 'context'}
          size='sm'
          className='h-7 min-w-0 flex-1 px-1 text-[10px]'
          onClick={() => setPanel('context')}
        >
          <BookOpen className='size-3 shrink-0' /> {t('chapterContext.title')}
        </Button>
        <Button
          variant={panel === 'agent' ? 'secondary' : 'ghost'}
          aria-pressed={panel === 'agent'}
          size='sm'
          className='h-7 flex-1 text-[10px]'
          onClick={() => setPanel('agent')}
        >
          <Bot className='size-3' /> {t('agent.title')}
        </Button>
        {issueCount > 0 && (
          <Button
            variant={panel === 'qc' ? 'secondary' : 'ghost'}
            aria-pressed={panel === 'qc'}
            size='sm'
            className='h-7 min-w-0 flex-1 px-1 text-[10px]'
            onClick={() => setPanel('qc')}
          >
            <ListChecks className='size-3 shrink-0' /> {t('qc.title')}{' '}
            <span className='rounded-full bg-amber-400 px-1.5 text-[9px] font-semibold text-black'>
              {issueCount}
            </span>
          </Button>
        )}
      </div>
      <div className={panel === 'qc' ? 'flex min-h-0 min-w-0 flex-1 flex-col' : 'hidden'}>
        <QcPanel />
      </div>
      <div className={panel === 'agent' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}>
        <AgentPanel />
      </div>
      {panel === 'context' && <ContextPanel />}
      <div className={panel === 'properties' ? 'min-h-0 flex-1' : 'hidden'}>
        <Inspector />
      </div>
    </aside>
  )
}
