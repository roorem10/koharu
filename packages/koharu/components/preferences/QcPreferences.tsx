'use client'

import { useMutation } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useQcSettings, qcSettingsKey } from '@/components/qc/data'
import { call } from '@/lib/backend'
import { queryClient } from '@/lib/queries'
import { commands } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Input } from '@koharu/ui/components/input'

import { PreferenceRow, PreferenceSection } from './PreferenceFields'

export function QcPreferences() {
  const { t } = useTranslation()
  const settings = useQcSettings()
  const [minimum, setMinimum] = useState('12')
  useEffect(() => {
    if (settings.data) setMinimum(String(settings.data.min_font_size))
  }, [settings.data])
  const mutation = useMutation({
    mutationFn: (minimum: number) => call(commands.saveQcSettings, { min_font_size: minimum }),
    onSuccess: (saved) => queryClient.setQueryData(qcSettingsKey, saved),
    onError: () => setMinimum(String(settings.data?.min_font_size ?? 12)),
  })
  return (
    <PreferenceSection title={t('qc.title')}>
      <PreferenceRow title={t('qc.minimumFontSize')}>
        <Input
          type='number'
          min={1}
          step={1}
          value={minimum}
          onChange={(event) => setMinimum(event.currentTarget.value)}
          aria-label={t('qc.minimumFontSize')}
          disabled={!settings.data || mutation.isPending}
          onKeyDown={(event) => {
            if (event.key === 'Enter') event.currentTarget.blur()
          }}
          onBlur={(event) => {
            const value = Number(event.currentTarget.value)
            if (Number.isFinite(value) && value > 0 && value !== settings.data?.min_font_size)
              mutation.mutate(value)
            else setMinimum(String(settings.data?.min_font_size ?? 12))
          }}
        />
      </PreferenceRow>
      {settings.isError && (
        <p className='py-2 text-xs text-destructive'>
          {t('qc.settingsFailed')}{' '}
          <Button size='sm' variant='ghost' onClick={() => void settings.refetch()}>
            {t('qc.retry')}
          </Button>
        </p>
      )}
    </PreferenceSection>
  )
}
