'use client'

import { useTranslation } from 'react-i18next'

import { useLetteringSettings, useSaveLetteringSettings } from '@/components/lettering/queries'
import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { type SfxMode } from '@koharu/bridge/protocol'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

const modes = ['skip', 'inside_bubbles', 'translate_all'] as const

export function SfxPreferences() {
  const { t } = useTranslation()
  const settings = useLetteringSettings()
  const save = useSaveLetteringSettings()
  return (
    <PreferenceSection title={t('lettering.sfxTitle')} description={t('lettering.sfxDescription')}>
      <PreferenceRow
        title={t('lettering.sfxMode')}
        description={
          settings.data
            ? t(`lettering.sfxHelp.${settings.data.sfx ?? 'skip'}`)
            : t(settings.isError ? 'lettering.loadError' : 'lettering.loading')
        }
      >
        <Select
          value={settings.data?.sfx ?? 'skip'}
          disabled={!settings.data || save.isPending}
          onValueChange={(sfx) => {
            if (settings.data && sfx) save.mutate({ ...settings.data, sfx: sfx as SfxMode })
          }}
        >
          <SelectTrigger className='w-full' aria-label={t('lettering.sfxMode')}>
            <SelectValue>{t(`lettering.sfxModes.${settings.data?.sfx ?? 'skip'}`)}</SelectValue>
          </SelectTrigger>
          <SelectContent>
            {modes.map((mode) => (
              <SelectItem key={mode} value={mode}>
                <span className='grid gap-0.5 whitespace-normal'>
                  <span>{t(`lettering.sfxModes.${mode}`)}</span>
                  <span className='text-[10px] text-muted-foreground'>
                    {t(`lettering.sfxHelp.${mode}`)}
                  </span>
                </span>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </PreferenceRow>
    </PreferenceSection>
  )
}
