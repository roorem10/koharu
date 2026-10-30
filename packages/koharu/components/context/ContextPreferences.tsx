'use client'

import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { call } from '@/lib/backend'
import { queryClient } from '@/lib/queries'
import { commands, type ReadWhen } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import { Input } from '@koharu/ui/components/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

import { contextSettingsKey, useChapterContext } from './data'
import { GlossaryDialog } from './GlossaryDialog'
import type { ContextSettings } from './types'

export function ContextPreferences() {
  const { t } = useTranslation()
  const chapter = useChapterContext().data
  const [open, setOpen] = useState(false)
  const settings = useQuery({
    queryKey: contextSettingsKey,
    queryFn: async () => (await call(commands.getContextSettings)) as ContextSettings,
  }).data
  const mutation = useMutation({
    mutationFn: (value: ContextSettings) => call(commands.saveContextSettings, value),
    onSuccess: (value) => queryClient.setQueryData(contextSettingsKey, value),
  })
  const save = (value: ContextSettings) => mutation.mutate(value)
  return (
    <PreferenceSection
      title={t('chapterContext.title')}
      description={t('chapterContext.settingsDescription')}
    >
      {settings && (
        <>
          <PreferenceRow title={t('chapterContext.readWhen')}>
            <Select
              value={settings.read_when}
              items={Object.fromEntries(
                (['missing', 'always', 'never'] as const).map((mode) => [
                  mode,
                  t(`chapterContext.readModes.${mode}`),
                ]),
              )}
              disabled={mutation.isPending}
              onValueChange={(value) =>
                value && save({ ...settings, read_when: value as ReadWhen })
              }
            >
              <SelectTrigger aria-label={t('chapterContext.readWhen')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {(['missing', 'always', 'never'] as const).map((value) => (
                  <SelectItem key={value} value={value}>
                    {t(`chapterContext.readModes.${value}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </PreferenceRow>
          <PreferenceRow
            title={t('chapterContext.sendImages')}
            description={t('chapterContext.sendImagesHint')}
          >
            <Checkbox
              aria-label={t('chapterContext.sendImages')}
              checked={settings.send_images}
              disabled={mutation.isPending}
              onCheckedChange={(checked) => save({ ...settings, send_images: checked })}
            />
          </PreferenceRow>
          {(['pages_per_request', 'previous_pages', 'agent_messages'] as const).map((key) => (
            <PreferenceRow key={key} title={t(`chapterContext.settings.${key}`)}>
              <Input
                type='number'
                aria-label={t(`chapterContext.settings.${key}`)}
                defaultValue={settings[key]}
                key={settings[key]}
                min={key === 'previous_pages' ? 0 : 1}
                max={key === 'pages_per_request' ? 40 : key === 'previous_pages' ? 2 : 200}
                disabled={mutation.isPending}
                onBlur={(event) => {
                  const value = Number(event.target.value)
                  if (
                    event.target.validity.valid &&
                    Number.isInteger(value) &&
                    value !== settings[key]
                  )
                    save({ ...settings, [key]: value })
                }}
              />
            </PreferenceRow>
          ))}
        </>
      )}
      <PreferenceRow title={t('chapterContext.glossary')}>
        <Button size='sm' variant='outline' onClick={() => setOpen(true)}>
          {t('chapterContext.openGlossary')}
        </Button>
      </PreferenceRow>
      {open && (
        <GlossaryDialog open onClose={() => setOpen(false)} seriesId={chapter?.series?.id} />
      )}
    </PreferenceSection>
  )
}
