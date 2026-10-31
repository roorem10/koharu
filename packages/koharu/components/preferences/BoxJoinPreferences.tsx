'use client'

import { useMutation, useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { queryClient } from '@/lib/queries'
import { commands } from '@koharu/bridge/protocol'
import { Switch } from '@koharu/ui/components/switch'

import { PreferenceRow, PreferenceSection } from './PreferenceFields'

const settingsKey = ['boxes-settings'] as const

export function BoxJoinPreferences() {
  const { t } = useTranslation()
  const settings = useQuery({ queryKey: settingsKey, queryFn: () => call(commands.getBoxSettings) })
  const save = useMutation({
    mutationFn: (join: boolean) => call(commands.saveBoxSettings, { join_in_bubble: join }),
    onSuccess: (value) => queryClient.setQueryData(settingsKey, value),
  })

  return (
    <PreferenceSection title={t('boxes.title')}>
      <PreferenceRow title={t('boxes.joinAutomatically')} description={t('boxes.joinExplanation')}>
        <Switch
          aria-label={t('boxes.joinAutomatically')}
          checked={settings.data?.join_in_bubble ?? true}
          disabled={!settings.data || save.isPending}
          onCheckedChange={(checked) => save.mutate(checked)}
        />
      </PreferenceRow>
    </PreferenceSection>
  )
}
