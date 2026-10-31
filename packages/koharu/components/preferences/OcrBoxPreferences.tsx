'use client'

import { useTranslation } from 'react-i18next'

import { ColorWell } from '@/components/controls/ColorWell'
import { FontPicker } from '@/components/controls/FontPicker'
import { useOcrBoxSettings, useSaveOcrBoxSettings } from '@/components/ocr-box/queries'
import { hexToRgba, rgbaToHex } from '@/lib/color'
import { useFonts } from '@/lib/queries'
import type { OcrBoxMode, OcrBoxSettings } from '@koharu/bridge/protocol'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

import { NumberField, PreferenceRow, PreferenceSection } from './PreferenceFields'

export function OcrBoxPreferences() {
  const { t } = useTranslation()
  const query = useOcrBoxSettings()
  const update = useSaveOcrBoxSettings()
  const fonts = useFonts()
  const settings = query.data
  const disabled = !settings || update.isPending
  const save = (change: Partial<OcrBoxSettings>) => {
    if (settings) update.mutate({ ...settings, ...change })
  }
  return (
    <PreferenceSection title={t('ocrBox.title')} description={t('ocrBox.modifierHint')}>
      {query.isError && (
        <p role='alert' className='py-3 text-xs text-destructive'>
          {t('ocrBox.settingsError')}
        </p>
      )}
      <PreferenceRow title={t('ocrBox.defaultMode')}>
        <Select
          value={settings?.default_mode ?? 'read_translate'}
          disabled={disabled}
          onValueChange={(mode) => {
            if (mode) save({ default_mode: mode as OcrBoxMode })
          }}
        >
          <SelectTrigger className='w-full' aria-label={t('ocrBox.defaultMode')}>
            <SelectValue>
              {t(`ocrBox.modes.${settings?.default_mode ?? 'read_translate'}`)}
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {(['read', 'read_translate', 'read_translate_clean'] as const).map((mode) => (
              <SelectItem key={mode} value={mode}>
                {t(`ocrBox.modes.${mode}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </PreferenceRow>
      <PreferenceRow title={t('ocrBox.subtitleFont')} description={t('ocrBox.defaultFont')}>
        <FontPicker
          value={settings?.subtitle_font_family ?? ''}
          families={fonts.data ?? []}
          disabled={disabled || fonts.isPending}
          ariaLabel={t('ocrBox.subtitleFont')}
          emptyLabel={t('ocrBox.defaultFont')}
          onChange={(font) => save({ subtitle_font_family: font || null })}
        />
      </PreferenceRow>
      <PreferenceRow title={t('ocrBox.subtitleSize')}>
        <fieldset disabled={disabled}>
          <NumberField
            label={t('ocrBox.sizePixels')}
            value={settings?.subtitle_size ?? 18}
            min={1}
            max={4096}
            step={1}
            onChange={(size) => {
              if (size !== null) save({ subtitle_size: size })
            }}
          />
        </fieldset>
      </PreferenceRow>
      <PreferenceRow title={t('ocrBox.subtitleFill')}>
        <ColorWell
          label={t('ocrBox.subtitleFill')}
          disabled={disabled}
          value={rgbaToHex(settings?.subtitle_fill ?? [0, 0, 0, 255])}
          onChange={(color) => save({ subtitle_fill: hexToRgba(color) })}
        />
      </PreferenceRow>
      <PreferenceRow title={t('ocrBox.subtitleOutline')}>
        <ColorWell
          label={t('ocrBox.subtitleOutline')}
          disabled={disabled}
          value={rgbaToHex(settings?.subtitle_outline ?? [255, 255, 255, 255])}
          onChange={(color) => save({ subtitle_outline: hexToRgba(color) })}
        />
      </PreferenceRow>
    </PreferenceSection>
  )
}
