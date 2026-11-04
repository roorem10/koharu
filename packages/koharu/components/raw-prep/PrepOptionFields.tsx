'use client'

import { useTranslation } from 'react-i18next'

import { NumberField } from '@/components/preferences/PreferenceFields'
import { Checkbox } from '@koharu/ui/components/checkbox'

import type { PrepOptions } from './data'

export function PrepOptionFields({
  value,
  onChange,
  disabled = false,
}: {
  value: PrepOptions
  onChange: (options: PrepOptions) => void
  disabled?: boolean
}) {
  const { t } = useTranslation()
  return (
    <fieldset disabled={disabled} className='grid gap-3'>
      <div className='grid grid-cols-2 items-center gap-4'>
        <label className='flex items-center gap-2 text-xs'>
          <Checkbox
            aria-label={t('rawPrep.levels')}
            checked={value.levels}
            disabled={disabled}
            onCheckedChange={(checked) => onChange({ ...value, levels: !!checked })}
          />
          {t('rawPrep.levels')}
        </label>
        <fieldset disabled={disabled || !value.levels}>
          <NumberField
            label={t('rawPrep.clip')}
            value={value.clip_percent}
            min={0}
            max={49.9}
            step={0.1}
            onChange={(clip_percent) =>
              clip_percent !== null && onChange({ ...value, clip_percent })
            }
          />
        </fieldset>
      </div>
      <div className='grid grid-cols-2 items-center gap-4'>
        <label className='flex items-center gap-2 text-xs'>
          <Checkbox
            aria-label={t('rawPrep.whiten')}
            checked={value.whiten}
            disabled={disabled}
            onCheckedChange={(checked) => onChange({ ...value, whiten: !!checked })}
          />
          {t('rawPrep.whiten')}
        </label>
        <fieldset disabled={disabled || !value.whiten}>
          <NumberField
            label={t('rawPrep.threshold')}
            value={value.threshold}
            min={0}
            max={255}
            step={1}
            onChange={(threshold) =>
              threshold !== null && onChange({ ...value, threshold: Math.round(threshold) })
            }
          />
        </fieldset>
      </div>
    </fieldset>
  )
}
