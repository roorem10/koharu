'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { PreferenceRow } from '@/components/preferences/PreferenceFields'
import { Button } from '@koharu/ui/components/button'
import { Popover, PopoverContent, PopoverTrigger } from '@koharu/ui/components/popover'

import { ReadingModelPicker } from './ReadingModelPicker'
import type { ContextSettings } from './types'

export function HardLineModelPreference({
  settings,
  disabled,
  onChange,
}: {
  settings: ContextSettings
  disabled: boolean
  onChange: (settings: ContextSettings) => void
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  return (
    <PreferenceRow title={t('secondPass.model')} description={t('secondPass.modelDescription')}>
      <div className='grid min-w-0 gap-2'>
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger
            aria-label={t('secondPass.model')}
            render={
              <Button
                variant='outline'
                size='sm'
                disabled={disabled}
                className='w-full min-w-0 justify-start'
              />
            }
          >
            <span className='truncate' title={settings.hard_line_model?.model ?? undefined}>
              {settings.hard_line_model?.model ?? t('chapterContext.sameModel')}
            </span>
          </PopoverTrigger>
          <PopoverContent className='w-80 max-w-[90vw]'>
            <ReadingModelPicker
              value={settings.hard_line_model}
              onBack={() => setOpen(false)}
              onSelect={(model) => {
                onChange({ ...settings, hard_line_model: model })
                setOpen(false)
              }}
            />
          </PopoverContent>
        </Popover>
        {settings.hard_line_model && (
          <Button
            size='sm'
            variant='ghost'
            disabled={disabled}
            onClick={() => onChange({ ...settings, hard_line_model: null })}
          >
            {t('chapterContext.sameModel')}
          </Button>
        )}
      </div>
    </PreferenceRow>
  )
}
