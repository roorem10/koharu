'use client'

import { useTranslation } from 'react-i18next'

import type { EntityId } from '@koharu/bridge/protocol'

import { useQcResults } from './data'
import { FindingRow } from './FindingRow'
import { FixQcButton, useQcFixHistory, useQcReview } from './fixes'

export function BoxQcSection({ layer }: { layer: EntityId }) {
  const { t } = useTranslation()
  const report = useQcResults().data
  const open = (report?.findings ?? []).filter((finding) => finding.layer === layer)
  const hidden = (report?.hidden ?? []).filter((finding) => finding.layer === layer)
  const review = (useQcReview().data ?? []).filter(
    (item) => item.kind !== 'problem' && item.finding.layer === layer,
  )
  const history = useQcFixHistory(layer).data ?? []
  return (
    <div className='grid min-w-0 gap-2'>
      {open.length > 0 && <FixQcButton scope={{ scope: 'boxes', layers: [layer] }} />}
      {!open.length && !hidden.length && !review.length && !history.length && (
        <p className='text-xs text-muted-foreground'>{t('qc.noBoxIssues')}</p>
      )}
      {open.map((finding, index) => (
        <FindingRow key={`open:${index}`} finding={finding} navigate={false} />
      ))}
      {review.map((item, index) => (
        <FindingRow key={`review:${index}`} finding={item.finding} navigate={false} review />
      ))}
      {history.map((record, index) => (
        <details key={index} className='rounded-md border border-border/80 p-2 text-xs'>
          <summary className='cursor-pointer break-words'>
            {t('qc.autoFixedReason', { reason: t(`qc.fixReasons.${record.action}`) })}
          </summary>
          <dl className='mt-2 grid gap-1 break-words'>
            {record.previous_source !== null && (
              <>
                <dt className='text-[10px] text-muted-foreground'>{t('qc.previousSource')}</dt>
                <dd className='whitespace-pre-wrap'>
                  {record.previous_source || t('qc.emptyText')}
                </dd>
              </>
            )}
            {record.previous_translation !== null && (
              <>
                <dt className='text-[10px] text-muted-foreground'>{t('qc.previousTranslation')}</dt>
                <dd className='whitespace-pre-wrap'>
                  {record.previous_translation || t('qc.emptyText')}
                </dd>
              </>
            )}
          </dl>
        </details>
      ))}
      {hidden.length > 0 && <p className='text-[10px] text-muted-foreground'>{t('qc.hidden')}</p>}
      {hidden.map((finding, index) => (
        <FindingRow key={`hidden:${index}`} finding={finding} hidden navigate={false} />
      ))}
    </div>
  )
}
