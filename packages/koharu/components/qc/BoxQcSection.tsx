'use client'

import { useTranslation } from 'react-i18next'

import type { EntityId } from '@koharu/bridge/protocol'

import { useQcResults } from './data'
import { FindingRow } from './FindingRow'

export function BoxQcSection({ layer }: { layer: EntityId }) {
  const { t } = useTranslation()
  const report = useQcResults().data
  const open = (report?.findings ?? []).filter((finding) => finding.layer === layer)
  const hidden = (report?.hidden ?? []).filter((finding) => finding.layer === layer)
  return (
    <div className='grid min-w-0 gap-2'>
      {!open.length && !hidden.length && (
        <p className='text-xs text-muted-foreground'>{t('qc.noBoxIssues')}</p>
      )}
      {open.map((finding, index) => (
        <FindingRow key={`open:${index}`} finding={finding} navigate={false} />
      ))}
      {hidden.length > 0 && <p className='text-[10px] text-muted-foreground'>{t('qc.hidden')}</p>}
      {hidden.map((finding, index) => (
        <FindingRow key={`hidden:${index}`} finding={finding} hidden navigate={false} />
      ))}
    </div>
  )
}
