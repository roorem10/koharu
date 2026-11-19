'use client'

import { ListChecks } from 'lucide-react'
import { useId, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { FindingRow } from '@/components/booharu/qc/FindingRow'
import { showLayer } from '@/lib/booharu/boxDetails'
import { qcFilters, qcGroups, useBooharuQc, type QcFilter, type QcGroup } from '@/lib/booharu/qc'
import { useProject } from '@/lib/queries'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import { ScrollArea } from '@koharu/ui/components/scroll-area'

/** Each filter's name. */
const filterNames = {
  all: 'booharu.qc.all',
  auto: 'booharu.qc.autoChecks',
  agent: 'booharu.qc.agent',
} as const satisfies Record<QcFilter, string>

/** The QC tab's name in the right pane, with its icon and its count of open findings on a badge in
 * Booharu's warning yellow, as Booharu 0.0.4 showed it. */
export function QcTabLabel({ count }: { count: number }) {
  const { t } = useTranslation()
  return (
    <>
      <ListChecks /> <span className='truncate'>{t('booharu.qc.title')}</span>{' '}
      <span className='shrink-0 rounded-full bg-[var(--booharu-warning,#dba54a)] px-1.5 text-[9px] leading-4 font-semibold text-black tabular-nums'>
        {count}
      </span>
    </>
  )
}

/** The QC tab: the boxes the quality check says need a look, as Booharu 0.0.4 listed them, checked
 * automatically after each run and edit. Its rows are grouped by page, in the project's page order,
 * under each page's number now; All, Auto checks and Agent list every finding, the automatic
 * checks', or the agent's flags; Show hidden lists the findings marked OK too, with Undo OK.
 * Pressing a row shows its page and selects its box, the tab staying. */
export function QcTab() {
  const { t } = useTranslation()
  const pages = useBooharuQc((state) => state.pages)
  const order = useProject().data?.pages
  const [filter, setFilter] = useState<QcFilter>('all')
  const [showHidden, setShowHidden] = useState(false)
  const groups = useMemo(
    () =>
      qcGroups(
        pages,
        (order ?? []).map((page) => page.id),
        filter,
        showHidden,
      ),
    [pages, order, filter, showHidden],
  )

  return (
    <section aria-label={t('booharu.qc.title')} className='flex min-h-0 min-w-0 flex-1 flex-col'>
      <div className='grid gap-2 border-b border-border/80 p-3'>
        <p className='text-[10px] leading-4 text-muted-foreground'>{t('booharu.qc.automatic')}</p>
        <div className='flex flex-wrap gap-1'>
          {qcFilters.map((value) => (
            <Button
              key={value}
              type='button'
              size='xs'
              variant={filter === value ? 'secondary' : 'ghost'}
              aria-pressed={filter === value}
              className='h-6 px-2 text-[10px]'
              onClick={() => setFilter(value)}
            >
              {t(filterNames[value])}
            </Button>
          ))}
        </div>
        <label className='flex items-center gap-2 text-[10px] text-muted-foreground'>
          <Checkbox checked={showHidden} onCheckedChange={setShowHidden} />
          {t('booharu.qc.showHidden')}
        </label>
      </div>
      <ScrollArea className='min-h-0 flex-1'>
        {groups.length === 0 ? (
          <p className='p-3 text-[11px] leading-4 text-muted-foreground'>
            {t('booharu.qc.noFindings')}
          </p>
        ) : (
          groups.map((group) => <PageFindings key={group.page} group={group} />)
        )}
      </ScrollArea>
    </section>
  )
}

/** A page's rows, under its number. */
function PageFindings({ group }: { group: QcGroup }) {
  const { t } = useTranslation()
  const heading = useId()
  return (
    <section aria-labelledby={heading} className='pb-2'>
      <h3
        id={heading}
        className='bg-[var(--surface-well)] px-3 py-1.5 text-[10px] leading-4 font-semibold'
      >
        {t('booharu.qc.page', { page: group.number })}
      </h3>
      {group.findings.map((finding, index) => (
        <FindingRow
          key={`${finding.layer}:${finding.check}:${index}`}
          finding={finding}
          onShow={() => void showLayer(finding.page, finding.layer).catch(() => undefined)}
        />
      ))}
    </section>
  )
}
