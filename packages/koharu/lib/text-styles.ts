import type { TFunction } from 'i18next'

import type { StyleGroup, TextStyle } from '@koharu/bridge/protocol'

import { rgbaToHex } from './color'

export const styleGroups: StyleGroup[] = [
  'font',
  'size',
  'fill',
  'outline',
  'outer_outline',
  'alignment',
  'writing_mode',
]
export const captureGroups = styleGroups.filter(
  (group) => group !== 'size' && group !== 'writing_mode',
)
export const emptyTextStyle: TextStyle = {
  font: null,
  size: null,
  fill: null,
  outline: null,
  outer_outline: null,
  alignment: null,
  writing_mode: null,
}

export function styleSummary(style: TextStyle, group: StyleGroup, t: TFunction): string {
  const value = style[group]
  if (value == null) return t('textStyles.notSet')
  switch (group) {
    case 'font':
      return [
        style.font?.preferred_font ?? t('inspector.auto'),
        style.font?.font_weight,
        style.font?.font_style && t(`inspector.fontStyles.${style.font.font_style}`),
      ]
        .filter(Boolean)
        .join(' · ')
    case 'size':
      return style.size?.auto_fit ? t('inspector.autoFit') : String(style.size?.size)
    case 'fill':
      return rgbaToHex(style.fill!)
    case 'outline':
      return `${style.outline?.stroke_width ?? 0} px · ${rgbaToHex(style.outline?.stroke_color ?? [255, 255, 255, 255])}`
    case 'outer_outline':
      return `${style.outer_outline?.outer_stroke_width ?? 0} px · ${rgbaToHex(style.outer_outline?.outer_stroke_color ?? [0, 0, 0, 255])}`
    case 'alignment':
      return style.alignment?.value
        ? t(`textStyles.alignments.${style.alignment.value}`)
        : t('inspector.auto')
    case 'writing_mode':
      return style.writing_mode?.value
        ? t(`inspector.${style.writing_mode.value.toLowerCase()}`)
        : t('inspector.auto')
  }
}
