'use client'

import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { call } from '@/lib/backend'
import { queryClient } from '@/lib/queries'
import { commands } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import { Input } from '@koharu/ui/components/input'
import { Popover, PopoverTrigger, PopoverContent } from '@koharu/ui/components/popover'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

import { contextSettingsKey, useChapterContext } from './data'
import { GlossaryDialog } from './GlossaryDialog'
import { ReadingModelPicker } from './ReadingModelPicker'
import type { ContextSettings } from './types'

export function ContextPreferences() {
  const { t } = useTranslation()
  const chapter = useChapterContext().data
  const [open, setOpen] = useState(false)
  const [pickerOpen, setPickerOpen] = useState(false)
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
          <PreferenceRow title={t('chapterContext.automaticReading')}>
            <Select
              value={settings.read_when === 'never' ? 'no' : 'yes'}
              items={{ yes: t('chapterContext.yes'), no: t('chapterContext.no') }}
              disabled={mutation.isPending}
              onValueChange={(value) =>
                value && save({ ...settings, read_when: value === 'yes' ? 'missing' : 'never' })
              }
            >
              <SelectTrigger aria-label={t('chapterContext.automaticReading')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='yes'>{t('chapterContext.yes')}</SelectItem>
                <SelectItem value='no'>{t('chapterContext.no')}</SelectItem>
              </SelectContent>
            </Select>
          </PreferenceRow>
          <PreferenceRow title={t('chapterContext.askBeforeReading')}>
            <Checkbox
              aria-label={t('chapterContext.askBeforeReading')}
              checked={settings.ask_before_reading}
              disabled={mutation.isPending}
              onCheckedChange={(checked) =>
                save({ ...settings, ask_before_reading: checked === true })
              }
            />
          </PreferenceRow>
          <PreferenceRow
            title={t('chapterContext.readingModel')}
            description={t('chapterContext.readerDownload')}
          >
            <div className='grid min-w-0 gap-2'>
              <Popover open={pickerOpen} onOpenChange={setPickerOpen}>
                <PopoverTrigger
                  render={
                    <Button
                      variant='outline'
                      size='sm'
                      disabled={mutation.isPending}
                      className='w-full min-w-0 justify-start'
                    />
                  }
                >
                  <span className='truncate'>
                    {settings.read_model?.model ?? t('chapterContext.sameModel')}
                  </span>
                </PopoverTrigger>
                <PopoverContent className='w-80 max-w-[90vw]'>
                  <ReadingModelPicker
                    value={settings.read_model}
                    onBack={() => setPickerOpen(false)}
                    onSelect={(model) => {
                      save({ ...settings, read_model: model })
                      setPickerOpen(false)
                    }}
                  />
                </PopoverContent>
              </Popover>
              {settings.read_model && (
                <Button
                  size='sm'
                  variant='ghost'
                  disabled={mutation.isPending}
                  onClick={() => save({ ...settings, read_model: null })}
                >
                  {t('chapterContext.sameModel')}
                </Button>
              )}
            </div>
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
