'use client'

import { useTranslation } from 'react-i18next'

import { useLetteringSettings, useSaveLetteringSettings } from '@/components/lettering/queries'
import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { type SfxInsideMode, type SfxOutsideMode } from '@koharu/bridge/protocol'
import { Checkbox } from '@koharu/ui/components/checkbox'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

export function SfxPreferences() {
  const { t } = useTranslation()
  const settings = useLetteringSettings()
  const save = useSaveLetteringSettings()
  return (
    <PreferenceSection title={t('lettering.sfxTitle')} description={t('lettering.sfxDescription')}>
      <PreferenceRow title={t('lettering.sfxInside')} description={t('lettering.sfxInsideHelp')}>
        <Select
          value={settings.data?.sfx_inside ?? 'translate'}
          disabled={!settings.data || save.isPending}
          onValueChange={(sfx) => {
            if (settings.data && sfx)
              save.mutate({ ...settings.data, sfx_inside: sfx as SfxInsideMode })
          }}
        >
          <SelectTrigger className='w-full' aria-label={t('lettering.sfxInside')}>
            <SelectValue>
              {t(`lettering.sfxActions.${settings.data?.sfx_inside ?? 'translate'}`)}
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {(['translate', 'skip'] as const).map((mode) => (
              <SelectItem key={mode} value={mode}>
                {t(`lettering.sfxActions.${mode}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </PreferenceRow>
      <PreferenceRow title={t('lettering.sfxOutside')} description={t('lettering.sfxOutsideHelp')}>
        <Select
          value={settings.data?.sfx_outside ?? 'skip'}
          disabled={!settings.data || save.isPending}
          onValueChange={(sfx) => {
            if (settings.data && sfx)
              save.mutate({ ...settings.data, sfx_outside: sfx as SfxOutsideMode })
          }}
        >
          <SelectTrigger className='w-full' aria-label={t('lettering.sfxOutside')}>
            <SelectValue>
              {t(`lettering.sfxActions.${settings.data?.sfx_outside ?? 'skip'}`)}
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {(['skip', 'subtitle', 'replace'] as const).map((mode) => (
              <SelectItem key={mode} value={mode}>
                {t(`lettering.sfxActions.${mode}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </PreferenceRow>
      <PreferenceRow
        title={t('lettering.sfxImportantOnly')}
        description={t('lettering.sfxImportantHelp')}
      >
        <Checkbox
          aria-label={t('lettering.sfxImportantOnly')}
          checked={settings.data?.sfx_important_only ?? false}
          disabled={!settings.data || save.isPending}
          onCheckedChange={(sfx_important_only) => {
            if (settings.data) save.mutate({ ...settings.data, sfx_important_only })
          }}
        />
      </PreferenceRow>
      {settings.isError && (
        <p role='alert' className='py-3 text-xs text-destructive'>
          {t('lettering.loadError')}
        </p>
      )}
    </PreferenceSection>
  )
}
