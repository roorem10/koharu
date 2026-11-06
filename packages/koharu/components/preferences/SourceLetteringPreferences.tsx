'use client'

import { useTranslation } from 'react-i18next'

import { useLetteringSettings, useSaveLetteringSettings } from '@/components/lettering/queries'
import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

export function SourceLetteringPreferences() {
  const { t } = useTranslation()
  const settings = useLetteringSettings()
  const save = useSaveLetteringSettings()
  const required = String(settings.data?.shout_signals_needed ?? 2)
  return (
    <PreferenceSection
      title={t('lettering.source.title')}
      description={t('lettering.source.description')}
    >
      <PreferenceRow
        title={t('lettering.source.signalsNeeded')}
        description={t('lettering.source.signalsHelp')}
      >
        <Select
          value={required}
          disabled={!settings.data || save.isPending}
          onValueChange={(value) => {
            if (settings.data && value !== null) {
              save.mutate({ ...settings.data, shout_signals_needed: Number(value) })
            }
          }}
        >
          <SelectTrigger className='w-full' aria-label={t('lettering.source.signalsNeeded')}>
            <SelectValue>{required}</SelectValue>
          </SelectTrigger>
          <SelectContent>
            {[1, 2, 3].map((count) => (
              <SelectItem key={count} value={String(count)}>
                {count}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {(settings.isError || save.isError) && (
          <p role='status' className='mt-2 text-[10px] leading-4 text-destructive'>
            {t('lettering.loadError')}
          </p>
        )}
      </PreferenceRow>
    </PreferenceSection>
  )
}
