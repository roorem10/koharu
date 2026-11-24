'use client'

import { useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  NumberField,
  PreferenceRow,
  PreferenceSection,
} from '@/components/preferences/PreferenceFields'
import {
  completeLevels,
  isClipPercent,
  isThreshold,
  MAX_CLIP_PERCENT,
  saveLevels,
  type Levels,
} from '@/lib/booharu/levels'
import { useBooharuPreferences } from '@/lib/booharu/preferences'
import { Switch } from '@koharu/ui/components/switch'

/** Fix page levels' section of Settings > Pipeline, the `[booharu-levels]` table: the values its
 * dialog starts from, Auto levels with how much it clips at each end, and Whiten paper with its
 * threshold. Each change saves the section at once, over what the file holds just before, as the
 * other Booharu settings do; once the last save ends, what the file holds shows, so after a failed
 * save, whose error shows as every failed command's does, the file's values show. A cleared or
 * half-typed number saves nothing. */
export function LevelsPreferences() {
  const { t } = useTranslation()
  const preferences = useBooharuPreferences()
  const [unsaved, setUnsaved] = useState<Levels | null>(null)
  /** Counts changes, so a save that ends knows whether a later change is still being saved. */
  const changes = useRef(0)
  const saved = preferences.data ? completeLevels(preferences.data['booharu-levels']) : undefined
  const levels = unsaved ?? saved

  const change = (values: Partial<Levels>) => {
    if (!levels) return
    changes.current += 1
    const last = changes.current
    setUnsaved({ ...levels, ...values })
    void saveLevels(values)
      .catch(() => undefined)
      .finally(() => {
        if (changes.current === last) setUnsaved(null)
      })
  }

  return (
    <PreferenceSection
      title={t('booharu.levels.title')}
      description={t('booharu.levels.settingsDescription')}
    >
      <PreferenceRow title={t('booharu.levels.autoLevels')} align='start'>
        <div className='grid grid-cols-[auto_minmax(0,1fr)] items-end gap-3'>
          <Switch
            aria-label={t('booharu.levels.autoLevels')}
            className='mb-2'
            checked={levels?.auto_levels ?? true}
            disabled={!levels}
            onCheckedChange={(auto_levels) => change({ auto_levels })}
          />
          <NumberField
            label={t('booharu.levels.clip')}
            value={levels?.clip_percent ?? null}
            min={0}
            max={MAX_CLIP_PERCENT}
            step={0.1}
            onChange={(clip_percent) => {
              if (isClipPercent(clip_percent) && clip_percent !== levels?.clip_percent)
                change({ clip_percent })
            }}
          />
        </div>
      </PreferenceRow>
      <PreferenceRow title={t('booharu.levels.whiten')} align='start'>
        <div className='grid grid-cols-[auto_minmax(0,1fr)] items-end gap-3'>
          <Switch
            aria-label={t('booharu.levels.whiten')}
            className='mb-2'
            checked={levels?.whiten ?? false}
            disabled={!levels}
            onCheckedChange={(whiten) => change({ whiten })}
          />
          <NumberField
            label={t('booharu.levels.threshold')}
            value={levels?.threshold ?? null}
            min={0}
            max={255}
            step={1}
            integer
            onChange={(threshold) => {
              const whole = threshold === null ? null : Math.round(threshold)
              if (isThreshold(whole) && whole !== levels?.threshold) change({ threshold: whole })
            }}
          />
        </div>
      </PreferenceRow>
    </PreferenceSection>
  )
}
