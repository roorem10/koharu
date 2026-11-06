'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import type { PageCheckFinding, PageCheckPage } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

import { applyPageCheckFix, navigatePageCheck, usePageCheckResults } from './page-check-data'

export function PageCheckResults() {
  const { t } = useTranslation()
  const report = usePageCheckResults().data
  const project = useProject().data
  if (!report?.pages.length) return null
  return (
    <section aria-label={t('pageCheck.title')} className='border-b border-border/80'>
      <h3 className='px-3 py-2 text-[11px] font-semibold'>{t('pageCheck.title')}</h3>
      {report.pages.map((page) => (
        <div key={page.page} className='pb-2'>
          <h4 className='bg-[var(--surface-well)] px-3 py-2 text-[10px] font-semibold break-words'>
            {page.page_label}
          </h4>
          {page.status === 'skipped' ? (
            <p className='px-3 py-2 text-[11px] text-muted-foreground'>{t('pageCheck.noVision')}</p>
          ) : (
            <>
              {page.revision !== project?.revision && (
                <p className='px-3 py-1 text-[10px] text-muted-foreground'>
                  {t('pageCheck.stale')}
                </p>
              )}
              {!page.findings.length && (
                <p className='px-3 py-2 text-[11px] text-muted-foreground'>
                  {t('pageCheck.noFindings')}
                </p>
              )}
              {page.findings.map((finding) => (
                <PageCheckRow
                  key={finding.id}
                  page={page}
                  finding={finding}
                  stale={page.revision !== project?.revision}
                />
              ))}
            </>
          )}
        </div>
      ))}
    </section>
  )
}

function PageCheckRow({
  page,
  finding,
  stale,
}: {
  page: PageCheckPage
  finding: PageCheckFinding
  stale: boolean
}) {
  const { t } = useTranslation()
  const [busy, setBusy] = useState(false)
  const running = useKoharuStore((state) =>
    Object.values(state.jobs).some((job) => job.state === 'running'),
  )
  const run = async (action: () => Promise<void>) => {
    setBusy(true)
    try {
      await action()
    } catch {
      /* call reports errors. */
    } finally {
      setBusy(false)
    }
  }
  return (
    <div className='grid gap-1 border-b border-border/40 px-3 py-2'>
      <button
        type='button'
        disabled={busy}
        onClick={() => void run(() => navigatePageCheck(page, finding))}
        className='min-w-0 rounded-md text-left hover:bg-foreground/5'
      >
        <span className='block text-[9px] text-muted-foreground'>{t('pageCheck.title')}</span>
        <span className='block text-[11px] font-medium break-words'>
          {t(`pageCheck.kinds.${finding.kind}`)}
        </span>
        <span className='block text-[11px] break-words'>{finding.detail}</span>
      </button>
      {finding.fix === 'outline' ? (
        <Button
          size='sm'
          variant='outline'
          className='h-auto min-h-6 justify-self-start px-2 py-1 text-[10px] whitespace-normal'
          disabled={busy || running || stale}
          onClick={() => void run(() => applyPageCheckFix(page, finding))}
        >
          {t('pageCheck.outline')}
        </Button>
      ) : (
        <span className='text-[10px] text-muted-foreground'>{t('pageCheck.reviewOnly')}</span>
      )}
    </div>
  )
}
