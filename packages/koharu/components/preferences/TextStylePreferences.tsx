'use client'

import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'

import { ColorWell } from '@/components/controls/ColorWell'
import { FontPicker } from '@/components/controls/FontPicker'
import { NumberField, PreferenceRow } from '@/components/preferences/PreferenceFields'
import { refreshTextStyles, saveTextStyles } from '@/lib/backend'
import { hexToRgba, rgbaToHex } from '@/lib/color'
import { useFonts } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { emptyTextStyle } from '@/lib/text-styles'
import type { TextStyle } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

export function TextStylePreferences() {
  const { t } = useTranslation()
  const styles = useKoharuStore((state) => state.textStyles)
  useEffect(() => {
    if (!styles) void refreshTextStyles().catch(() => undefined)
  }, [styles])
  if (!styles) return <p role='status'>{t('common.loading')}</p>
  const style = styles.default_style ?? emptyTextStyle
  return (
    <section aria-labelledby='default-text-style'>
      <div className='mb-3 flex items-start justify-between gap-4'>
        <div>
          <h3 id='default-text-style' className='text-[12px] font-semibold'>
            {t('textStyles.defaultTitle')}
          </h3>
          <p className='mt-1 text-[11px] text-muted-foreground'>{t('textStyles.defaultHelp')}</p>
        </div>
        <Button
          size='sm'
          variant='outline'
          disabled={Object.values(style).every((value) => value == null)}
          onClick={() =>
            void saveTextStyles({ ...styles, default_style: { ...emptyTextStyle } }).catch(
              () => undefined,
            )
          }
        >
          {t('textStyles.clearAll')}
        </Button>
      </div>
      <div className='divide-y rounded-xl border bg-[var(--surface-panel)] px-5'>
        <TextStyleFields
          style={style}
          onChange={(default_style) =>
            void saveTextStyles({ ...styles, default_style }).catch(() => undefined)
          }
        />
      </div>
    </section>
  )
}

function TextStyleFields({
  style,
  onChange,
}: {
  style: TextStyle
  onChange: (style: TextStyle) => void
}) {
  const { t } = useTranslation()
  const fonts = useFonts()
  const clear = (group: 'font' | 'fill' | 'outline') => (
    <Button
      size='sm'
      variant='ghost'
      disabled={style[group] == null}
      aria-label={t('textStyles.clearGroup', { group: t(`textStyles.groups.${group}`) })}
      onClick={() => onChange({ ...style, [group]: null })}
    >
      {t('textStyles.clear')}
    </Button>
  )
  return (
    <>
      <PreferenceRow title={t('textStyles.groups.font')}>
        <div className='flex min-w-0 items-center gap-2'>
          <div className='min-w-0 flex-1'>
            <FontPicker
              value={style.font?.preferred_font ?? ''}
              families={fonts.data ?? []}
              ariaLabel={t('textStyles.defaultFont')}
              emptyLabel={t('textStyles.notSet')}
              onChange={(family) =>
                onChange({
                  ...style,
                  font: family
                    ? { preferred_font: family, font_weight: null, font_style: null }
                    : null,
                })
              }
            />
          </div>
          {clear('font')}
        </div>
      </PreferenceRow>
      <PreferenceRow title={t('textStyles.groups.fill')}>
        <div className='flex items-center gap-2'>
          <ColorWell
            label={t('textStyles.groups.fill')}
            value={style.fill ? rgbaToHex(style.fill) : null}
            allowTransparent
            emptyLabel={t('textStyles.notSet')}
            onChange={(value) => onChange({ ...style, fill: value ? hexToRgba(value) : null })}
          />
          <span className='min-w-0 flex-1 text-[11px] text-muted-foreground'>
            {style.fill ? rgbaToHex(style.fill) : t('textStyles.notSet')}
          </span>
          {clear('fill')}
        </div>
      </PreferenceRow>
      <PreferenceRow title={t('textStyles.groups.outline')}>
        <div className='flex items-end gap-2'>
          <ColorWell
            label={t('textStyles.outlineColor')}
            value={style.outline?.stroke_color ? rgbaToHex(style.outline.stroke_color) : null}
            allowTransparent
            emptyLabel={t('textStyles.notSet')}
            onChange={(value) =>
              onChange({
                ...style,
                outline: value
                  ? {
                      stroke_width: 1.5,
                      stroke_method: null,
                      stroke_position: null,
                      stroke_join: null,
                      ...style.outline,
                      stroke_color: hexToRgba(value),
                    }
                  : null,
              })
            }
          />
          <div className='min-w-0 flex-1'>
            <NumberField
              label={t('textStyles.outlineWidth')}
              value={style.outline?.stroke_width ?? null}
              min={0}
              step={0.5}
              onChange={(width) =>
                onChange({
                  ...style,
                  outline:
                    width == null
                      ? null
                      : {
                          stroke_color: [255, 255, 255, 255],
                          stroke_method: null,
                          stroke_position: null,
                          stroke_join: null,
                          ...style.outline,
                          stroke_width: width,
                        },
                })
              }
            />
          </div>
          {clear('outline')}
        </div>
        {style.outline == null && (
          <p className='mt-1 text-[10px] text-muted-foreground'>{t('textStyles.notSet')}</p>
        )}
      </PreferenceRow>
    </>
  )
}
