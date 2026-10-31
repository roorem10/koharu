'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { usePages } from '@/lib/queries'
import type { QcFinding } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import { ScrollArea } from '@koharu/ui/components/scroll-area'

import { useQcResults } from './data'
import { FindingRow } from './FindingRow'

export function QcPanel() {
  const { t } = useTranslation()
  const report = useQcResults()
  const pages = usePages(false).data ?? []
  const [showHidden, setShowHidden] = useState(false)
  const [filter, setFilter] = useState<'all' | 'auto' | 'agent'>('all')
  const matches = (finding: QcFinding) =>
    filter === 'all' || (finding.flag?.kind === 'review' ? filter === 'agent' : filter === 'auto')
  const entries = [
    ...(report.data?.findings ?? []).filter(matches).map((finding) => ({ finding, hidden: false })),
    ...(showHidden ? (report.data?.hidden ?? []) : [])
      .filter(matches)
      .map((finding) => ({ finding, hidden: true })),
  ]
  const groups = new Map<string, typeof entries>()
  for (const entry of entries) {
    const group = groups.get(entry.finding.page) ?? []
    group.push(entry)
    groups.set(entry.finding.page, group)
  }
  return (
    <section className='flex min-h-0 min-w-0 flex-1 flex-col' aria-label={t('qc.title')}>
      <div className='grid gap-2 border-b border-border/80 p-3'>
        <p className='text-[10px] text-muted-foreground'>{t('qc.automatic')}</p>
        <div className='flex flex-wrap gap-1'>
          {(['all', 'auto', 'agent'] as const).map((value) => (
            <Button
              key={value}
              size='sm'
              variant={filter === value ? 'secondary' : 'ghost'}
              className='h-6 px-2 text-[10px]'
              aria-pressed={filter === value}
              onClick={() => setFilter(value)}
            >
              {t(value === 'all' ? 'qc.all' : value === 'auto' ? 'qc.autoChecks' : 'qc.agent')}
            </Button>
          ))}
        </div>
        <label className='flex items-center gap-2 text-[10px] text-muted-foreground'>
          <Checkbox
            checked={showHidden}
            onCheckedChange={(checked) => setShowHidden(checked === true)}
          />
          {t('qc.showHidden')}
        </label>
      </div>
      <ScrollArea className='min-h-0 flex-1'>
        {!entries.length && (
          <p className='p-3 text-[11px] text-muted-foreground'>{t('qc.noFindings')}</p>
        )}
        {[...groups.entries()]
          .sort(([left], [right]) => {
            const a = pages.findIndex((page) => page.id === left),
              b = pages.findIndex((page) => page.id === right)
            return (a < 0 ? Infinity : a) - (b < 0 ? Infinity : b)
          })
          .map(([page, findings]) => (
            <div key={page} className='pb-2'>
              <h3
                className='truncate bg-[var(--surface-well)] px-3 py-2 text-[10px] font-semibold'
                title={findings[0]!.finding.page_label}
              >
                {findings[0]!.finding.page_label}
              </h3>
              {findings.map(({ finding, hidden }, index) => (
                <FindingRow
                  key={`${finding.layer}:${finding.check}:${index}:${hidden}`}
                  finding={finding}
                  hidden={hidden}
                />
              ))}
            </div>
          ))}
      </ScrollArea>
    </section>
  )
}
