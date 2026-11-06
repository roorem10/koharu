'use client'

import { BookOpen, Bot, ListChecks, SlidersHorizontal } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { ContextPanel } from '@/components/context/ContextPanel'
import { discussLineEvent, openContextEvent } from '@/components/context/events'
import { AgentPanel } from '@/components/editor/AgentPanel'
import { Inspector } from '@/components/editor/Inspector'
import { useAutomaticQc, useQcResults } from '@/components/qc/data'
import { useQcReview } from '@/components/qc/fixes'
import { QcPanel } from '@/components/qc/QcPanel'
import { invalidateLineLabels } from '@/lib/line-labels'
import { useKoharuStore } from '@/lib/store'
import { Button } from '@koharu/ui/components/button'

export function RightSidebar() {
  const { t } = useTranslation()
  useAutomaticQc()
  const review = useQcReview().data ?? []
  const issueCount =
    (useQcResults().data?.findings.length ?? 0) +
    review.filter((item) => item.kind !== 'problem').length
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

  // In a narrow window the four tabs share too little width for their labels, so
  // labels hide below 22rem and the icons (with tooltips) remain.
  const tab = 'h-7 min-w-0 flex-1 gap-1 overflow-hidden px-1 text-[10px]'
  const label = 'hidden truncate @[22rem]:inline'
  return (
    <aside className='flex h-full min-h-0 min-w-0 flex-col bg-[var(--surface-panel)]'>
      <div
        role='group'
        aria-label={t('qc.editorPanels')}
        className='@container flex h-10 shrink-0 items-center gap-1 border-b border-border/80 px-2.5'
      >
        <Button
          variant={panel === 'properties' ? 'secondary' : 'ghost'}
          aria-pressed={panel === 'properties'}
          size='sm'
          className={tab}
          title={t('agent.properties')}
          onClick={() => setPanel('properties')}
        >
          <SlidersHorizontal className='size-3 shrink-0' />
          <span className={label}>{t('agent.properties')}</span>
        </Button>
        <Button
          variant={panel === 'context' ? 'secondary' : 'ghost'}
          aria-pressed={panel === 'context'}
          size='sm'
          className={tab}
          title={t('chapterContext.title')}
          onClick={() => setPanel('context')}
        >
          <BookOpen className='size-3 shrink-0' />
          <span className={label}>{t('chapterContext.title')}</span>
        </Button>
        <Button
          variant={panel === 'agent' ? 'secondary' : 'ghost'}
          aria-pressed={panel === 'agent'}
          size='sm'
          className={tab}
          title={t('agent.title')}
          onClick={() => setPanel('agent')}
        >
          <Bot className='size-3 shrink-0' />
          <span className={label}>{t('agent.title')}</span>
        </Button>
        {issueCount > 0 && (
          <Button
            variant={panel === 'qc' ? 'secondary' : 'ghost'}
            aria-pressed={panel === 'qc'}
            size='sm'
            className={tab}
            title={t('qc.title')}
            onClick={() => setPanel('qc')}
          >
            <ListChecks className='size-3 shrink-0' />
            <span className={label}>{t('qc.title')}</span>{' '}
            <span className='shrink-0 rounded-full bg-amber-400 px-1.5 text-[9px] font-semibold text-black'>
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
