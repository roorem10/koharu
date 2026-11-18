'use client'

import { useMutation } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { saveBooharuSection, useBooharuPreferences } from '@/lib/booharu/preferences'
import type { OcrBoxMode } from '@koharu/ipc/protocol'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

const modes = [
  'read',
  'read_translate',
  'read_translate_clean',
] as const satisfies readonly OcrBoxMode[]

/** The O tool's section of Settings > Pipeline, the `[booharu-ocr-box]` table: what a box drawn
 * with it goes through when neither Alt nor Shift picks another mode. A choice is saved at once. */
export function OcrBoxPreferences() {
  const { t } = useTranslation()
  const preferences = useBooharuPreferences()
  const save = useMutation({
    mutationFn: (mode: OcrBoxMode) =>
      saveBooharuSection('booharu-ocr-box', (section) => ({ ...section, default_mode: mode })),
  })
  // The choice being saved shows at once, then what the file holds once the save ends.
  const mode = save.isPending ? save.variables : preferences.data?.['booharu-ocr-box'].default_mode
  const names = Object.fromEntries(
    modes.map((choice) => [choice, t(`booharu.ocrBox.modes.${choice}`)]),
  )

  return (
    <PreferenceSection title={t('tools.ocr_box')} description={t('booharu.ocrBox.description')}>
      <PreferenceRow
        title={t('booharu.ocrBox.defaultMode')}
        description={t('booharu.ocrBox.defaultModeDescription')}
      >
        <Select
          value={mode ?? null}
          items={names}
          disabled={!preferences.data}
          onValueChange={(next) => next && save.mutate(next as OcrBoxMode)}
        >
          <SelectTrigger
            aria-label={t('booharu.ocrBox.defaultMode')}
            className='h-8 w-full text-[11px]'
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {modes.map((choice) => (
              <SelectItem key={choice} value={choice}>
                {names[choice]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </PreferenceRow>
    </PreferenceSection>
  )
}
