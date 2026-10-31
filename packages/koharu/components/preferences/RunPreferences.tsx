'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  PreferenceRow,
  PreferenceSection,
  NumberField,
} from '@/components/preferences/PreferenceFields'
import type { SavedRunSettings } from '@/components/runs/data'
import { presetLabel, useRunSettings, useSaveRunSettings } from '@/components/runs/data'
import { pipelineStages } from '@/lib/store'
import { type OnPageFailure, type RunOrder } from '@koharu/bridge/protocol'
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

export function RunPreferences() {
  const { t } = useTranslation()
  const settings = useRunSettings()
  const save = useSaveRunSettings()
  const [name, setName] = useState('')
  const value = settings.data
  const update = (next: SavedRunSettings) => save.mutate(next)
  return (
    <PreferenceSection title={t('runs.title')} description={t('runs.description')}>
      {!value ? (
        <p role='status' className='py-3 text-xs'>
          {t(settings.isError ? 'runs.loadError' : 'runs.loading')}
        </p>
      ) : (
        <fieldset disabled={save.isPending}>
          <PreferenceRow title={t('runs.retries')} description={t('runs.retriesHelp')}>
            <NumberField
              label={t('runs.retries')}
              min={0}
              max={5}
              step={1}
              value={value.retries}
              onChange={(retries) =>
                retries !== null && update({ ...value, retries: Math.round(retries) })
              }
            />
          </PreferenceRow>
          <PreferenceRow title={t('runs.onFailure')}>
            <Select
              value={value.on_page_failure}
              onValueChange={(mode) =>
                mode && update({ ...value, on_page_failure: mode as OnPageFailure })
              }
            >
              <SelectTrigger className='w-full' aria-label={t('runs.onFailure')}>
                <SelectValue>{t(`runs.${value.on_page_failure}`)}</SelectValue>
              </SelectTrigger>
              <SelectContent>
                {(['continue_with_other_pages', 'stop_run'] as const).map((mode) => (
                  <SelectItem key={mode} value={mode}>
                    {t(`runs.${mode}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </PreferenceRow>
          <PreferenceRow title={t('runs.order')} description={t('runs.orderHelp')}>
            <Select
              value={value.order}
              onValueChange={(order) => order && update({ ...value, order: order as RunOrder })}
            >
              <SelectTrigger className='w-full' aria-label={t('runs.order')}>
                <SelectValue>{t(`runs.${value.order}`)}</SelectValue>
              </SelectTrigger>
              <SelectContent>
                {(['overlapped', 'stage_at_a_time'] as const).map((order) => (
                  <SelectItem key={order} value={order}>
                    {t(`runs.${order}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </PreferenceRow>
          <PreferenceRow title={t('runs.finishSound')}>
            <Checkbox
              aria-label={t('runs.finishSound')}
              checked={value.finish_sound}
              onCheckedChange={(checked) => update({ ...value, finish_sound: !!checked })}
            />
          </PreferenceRow>
          <PreferenceRow title={t('runs.presets')} align='start'>
            <div className='grid gap-3'>
              {value.presets.map((preset, index) => (
                <div key={index} className='grid gap-2 rounded-lg border p-2'>
                  <div className='flex gap-1'>
                    <PresetName
                      key={preset.name}
                      name={preset.name}
                      label={t('runs.presetName')}
                      disabled={save.isPending}
                      onChange={(name) =>
                        update({
                          ...value,
                          presets: value.presets.map((p, i) => (i === index ? { ...p, name } : p)),
                        })
                      }
                    />
                    <Button
                      size='sm'
                      variant='ghost'
                      onClick={() =>
                        update({ ...value, presets: value.presets.filter((_, i) => i !== index) })
                      }
                    >
                      {t('runs.delete')}
                    </Button>
                  </div>
                  <div className='grid grid-cols-2 gap-2'>
                    {pipelineStages.map((stage) => (
                      <label key={stage} className='flex items-center gap-2 text-xs'>
                        <Checkbox
                          aria-label={`${presetLabel(preset.name, t)}: ${t(`phase.${stage}`)}`}
                          checked={preset.stages.includes(stage)}
                          disabled={preset.stages.length === 1 && preset.stages.includes(stage)}
                          onCheckedChange={(checked) =>
                            update({
                              ...value,
                              presets: value.presets.map((p, i) =>
                                i === index
                                  ? {
                                      ...p,
                                      stages: pipelineStages.filter((s) =>
                                        s === stage ? !!checked : p.stages.includes(s),
                                      ),
                                    }
                                  : p,
                              ),
                            })
                          }
                        />
                        {t(`phase.${stage}`)}
                      </label>
                    ))}
                  </div>
                </div>
              ))}
              <div className='flex gap-1'>
                <Input
                  value={name}
                  maxLength={256}
                  aria-label={t('runs.newPresetName')}
                  placeholder={t('runs.newPresetName')}
                  onChange={(e) => setName(e.currentTarget.value)}
                />
                <Button
                  variant='outline'
                  disabled={!name.trim() || value.presets.length >= 100}
                  onClick={() => {
                    update({
                      ...value,
                      presets: [
                        ...value.presets,
                        { name: name.trim(), stages: [...pipelineStages] },
                      ],
                    })
                    setName('')
                  }}
                >
                  {t('runs.add')}
                </Button>
              </div>
            </div>
          </PreferenceRow>
        </fieldset>
      )}
    </PreferenceSection>
  )
}

function PresetName({
  name,
  label,
  disabled,
  onChange,
}: {
  name: string
  label: string
  disabled: boolean
  onChange: (name: string) => void
}) {
  return (
    <Input
      key={name}
      defaultValue={name}
      disabled={disabled}
      aria-label={label}
      maxLength={256}
      onBlur={(event) => {
        const draft = event.currentTarget.value.trim()
        if (draft && draft !== name) onChange(draft)
        else event.currentTarget.value = name
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter') e.currentTarget.blur()
        if (e.key === 'Escape') {
          e.currentTarget.value = name
          e.currentTarget.blur()
        }
      }}
    />
  )
}
