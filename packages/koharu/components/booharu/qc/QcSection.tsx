'use client'

import { useId } from 'react'
import { useTranslation } from 'react-i18next'

import type { BoxDetailsProps } from '@/components/booharu/details/sections'
import { FindingRow } from '@/components/booharu/qc/FindingRow'
import { useLayerFindings } from '@/lib/booharu/qc'

/** Box details' QC section, for every text: what the quality check found on it, as the QC tab lists
 * it, the open findings with Mark OK and the agent's flags with Dismiss, then those marked OK while
 * the text stays as it is, with Undo OK; or that it found nothing, which it also says of a locked
 * text, as the check leaves those alone. */
export function QcSection({ page, box: { layer } }: BoxDetailsProps) {
  const { t } = useTranslation()
  const heading = useId()
  const findings = useLayerFindings(page, layer.id)
  if (!findings.length) {
    return (
      <p className='text-[11px] leading-4 text-muted-foreground'>
        {t('booharu.details.noBoxIssues')}
      </p>
    )
  }

  const open = findings.filter((finding) => !finding.hidden)
  const hidden = findings.filter((finding) => finding.hidden)
  return (
    <div className='grid min-w-0'>
      {open.map((finding, index) => (
        <FindingRow key={`open:${finding.check}:${index}`} finding={finding} />
      ))}
      {hidden.length > 0 && (
        <section aria-labelledby={heading} className='grid min-w-0'>
          <h4 id={heading} className='px-2 pt-2 pb-0.5 text-[10px] leading-4 text-muted-foreground'>
            {t('booharu.details.hidden')}
          </h4>
          {hidden.map((finding, index) => (
            <FindingRow key={`hidden:${finding.check}:${index}`} finding={finding} />
          ))}
        </section>
      )}
    </div>
  )
}
