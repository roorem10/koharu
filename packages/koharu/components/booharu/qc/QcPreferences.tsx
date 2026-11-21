'use client'

import { useMutation } from '@tanstack/react-query'
import { Minus, Plus } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { writeBooharuSection, useBooharuPreferences } from '@/lib/booharu/preferences'
import { MIN_SIZE_PERCENT, minSizePercent, minSizeShare } from '@/lib/booharu/qc'
import {
  NumberField,
  NumberFieldDecrement,
  NumberFieldGroup,
  NumberFieldIncrement,
  NumberFieldInput,
} from '@koharu/ui/components/number-field'

/** The quality check's section of Settings > Pipeline, the `[booharu-qc]` table: the smallest
 * readable text, as a percentage of the page's shorter side, under which drawn text is too small.
 * A value is saved once the field commits it, as it loses focus or a step ends; a cleared field
 * saves nothing and shows the saved value again, and the field keeps a typed value within 0.1% to
 * 10%. The check has no switch: it always runs. */
export function QcPreferences() {
  const { t } = useTranslation()
  const preferences = useBooharuPreferences()
  // The value while the field is edited; undefined while it isn't.
  const [editing, setEditing] = useState<number | null | undefined>(undefined)
  const save = useMutation({
    mutationFn: (min_size: number) =>
      writeBooharuSection('booharu-qc', (section) => ({ ...section, min_size })),
  })
  const saved = preferences.data ? minSizePercent(preferences.data['booharu-qc']) : null
  // The value being saved shows at once, then what the file holds once the save ends.
  const shown = save.isPending ? minSizePercent({ min_size: save.variables }) : saved

  return (
    <PreferenceSection
      title={t('booharu.qc.settings.title')}
      description={t('booharu.qc.settings.description')}
    >
      <PreferenceRow
        title={t('booharu.qc.settings.minSize')}
        description={t('booharu.qc.settings.minSizeHint')}
      >
        <div className='flex items-center gap-1.5'>
          <NumberField
            className='w-28'
            value={editing === undefined ? shown : editing}
            min={MIN_SIZE_PERCENT.min}
            max={MIN_SIZE_PERCENT.max}
            step={MIN_SIZE_PERCENT.step}
            smallStep={MIN_SIZE_PERCENT.step}
            format={{ maximumFractionDigits: 2 }}
            disabled={!preferences.data}
            onValueChange={setEditing}
            onValueCommitted={(percent) => {
              setEditing(undefined)
              const share = minSizeShare(percent)
              if (share === null || percent === shown) return
              save.mutate(share)
            }}
          >
            <NumberFieldGroup>
              <NumberFieldDecrement aria-label={t('booharu.qc.settings.smaller')}>
                <Minus />
              </NumberFieldDecrement>
              <NumberFieldInput aria-label={t('booharu.qc.settings.minSize')} />
              <NumberFieldIncrement aria-label={t('booharu.qc.settings.larger')}>
                <Plus />
              </NumberFieldIncrement>
            </NumberFieldGroup>
          </NumberField>
          <span aria-hidden='true' className='text-[11px] text-muted-foreground'>
            %
          </span>
        </div>
      </PreferenceRow>
    </PreferenceSection>
  )
}
