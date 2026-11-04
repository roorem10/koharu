'use client'

import { useTranslation } from 'react-i18next'

import type { PrepResult } from '@koharu/bridge/protocol'

export function PrepResultView({ result }: { result: PrepResult }) {
  const { t } = useTranslation()
  return (
    <div role='status' className='grid gap-2 text-xs'>
      <p>{t('rawPrep.prepared', { count: result.changed })}</p>
      {result.skipped.length > 0 && (
        <>
          <p>{t('rawPrep.skipped', { count: result.skipped.length })}</p>
          <ul className='max-h-52 space-y-1 overflow-auto'>
            {result.skipped.map((page) => (
              <li key={page.page} className='break-all'>
                {t('rawPrep.skipDetail', {
                  page: page.label,
                  reason: t(`rawPrep.reason.${page.reason}`),
                })}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  )
}
