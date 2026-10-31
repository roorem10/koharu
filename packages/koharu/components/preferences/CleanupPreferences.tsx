'use client'

import { useMutation } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import {
  cleanupSettingsKey,
  defaultCleanupSettings,
  useCleanupSettings,
} from '@/components/cleanup/settings'
import { call } from '@/lib/backend'
import { queryClient } from '@/lib/queries'
import { commands, type CleanupSettings } from '@koharu/bridge/protocol'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

import { NumberField, PreferenceRow, PreferenceSection } from './PreferenceFields'

export function CleanupPreferences() {
  const { t } = useTranslation()
  const settings = useCleanupSettings()
  const value = settings.data ?? defaultCleanupSettings
  const save = useMutation({
    mutationFn: (next: CleanupSettings) => call(commands.saveCleanupSettings, next),
    onSuccess: (next) =>
      queryClient.setQueryData(cleanupSettingsKey, { ...defaultCleanupSettings, ...next }),
  })
  const disabled = !settings.data || save.isPending
  return (
    <PreferenceSection title={t('cleanup.title')}>
      <PreferenceRow title={t('cleanup.removeMode')} description={t('cleanup.queueHelp')}>
        <Select
          value={value.remove_mode}
          disabled={disabled}
          items={{ queue: t('cleanup.queueMode'), immediate: t('cleanup.immediateMode') }}
          onValueChange={(remove_mode) => {
            if (remove_mode)
              save.mutate({ ...value, remove_mode: remove_mode as CleanupSettings['remove_mode'] })
          }}
        >
          <SelectTrigger aria-label={t('cleanup.removeMode')} className='h-8 text-xs'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value='queue'>{t('cleanup.queueMode')}</SelectItem>
            <SelectItem value='immediate'>{t('cleanup.immediateMode')}</SelectItem>
          </SelectContent>
        </Select>
      </PreferenceRow>
      <PreferenceRow title={t('cleanup.paintTarget')} description={t('cleanup.targetHelp')}>
        <Select
          value={value.paint_target}
          disabled={disabled}
          items={{ cleanup: t('cleanup.cleanupLayer'), paint_layer: t('cleanup.paintLayer') }}
          onValueChange={(paint_target) => {
            if (paint_target)
              save.mutate({
                ...value,
                paint_target: paint_target as CleanupSettings['paint_target'],
              })
          }}
        >
          <SelectTrigger aria-label={t('cleanup.paintTarget')} className='h-8 text-xs'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value='cleanup'>{t('cleanup.cleanupLayer')}</SelectItem>
            <SelectItem value='paint_layer'>{t('cleanup.paintLayer')}</SelectItem>
          </SelectContent>
        </Select>
      </PreferenceRow>
      <PreferenceRow title={t('cleanup.growMask')} description={t('cleanup.growHelp')}>
        <fieldset disabled={disabled}>
          <NumberField
            label={t('cleanup.growPixels')}
            value={value.grow_px}
            min={0}
            max={16}
            step={1}
            onChange={(grow_px) => {
              if (grow_px !== null) save.mutate({ ...value, grow_px: Math.round(grow_px) })
            }}
          />
        </fieldset>
      </PreferenceRow>
    </PreferenceSection>
  )
}
