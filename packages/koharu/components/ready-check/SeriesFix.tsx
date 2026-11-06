'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { refreshContext } from '@/components/context/data'
import { SeriesPicker } from '@/components/context/SeriesPicker'
import type { Series } from '@/components/context/types'
import { call } from '@/lib/backend'
import { commands } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

import { refreshReadyCheck } from './data'

export function SeriesFix({
  projectName,
  seriesId,
}: {
  projectName: string
  seriesId: string | null
}) {
  const { t } = useTranslation()
  const [busy, setBusy] = useState(false)
  const link = async (id: string) => {
    setBusy(true)
    try {
      await call(commands.setProjectSeries, id)
      await refreshContext()
      await refreshReadyCheck()
    } finally {
      setBusy(false)
    }
  }
  return (
    <fieldset disabled={busy} className='grid min-w-0 gap-2'>
      <SeriesPicker value={seriesId} onChange={link} />
      {!seriesId && (
        <Button
          className='h-auto min-h-8 break-words whitespace-normal'
          variant='outline'
          size='sm'
          disabled={busy}
          onClick={() => {
            setBusy(true)
            void call(commands.createSeries, projectName)
              .then((series) => link((series as Series).id))
              .catch(() => undefined)
              .finally(() => setBusy(false))
          }}
        >
          {t('readyCheck.createSeries', { name: projectName })}
        </Button>
      )}
    </fieldset>
  )
}
