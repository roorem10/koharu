'use client'

import { useTranslation } from 'react-i18next'

import { ColorWell } from '@/components/controls/ColorWell'
import { Field, NumericControl, SelectControl } from '@/components/redesign/Controls'
import { isMixed } from '@/components/redesign/useTypography'
import { hexToRgba, rgbaToHex } from '@/lib/color'
import type { FillGradient, Typography } from '@koharu/bridge/protocol'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

import type { ApplyTypography } from './SpacingControls'

export function gradientFor(value: Typography): FillGradient {
  return (
    value.fill_gradient ?? {
      kind: 'Linear',
      angle_degrees: 0,
      stops: [
        { offset: 0, color: value.color ?? [0, 0, 0, 255] },
        { offset: 1, color: [255, 255, 255, 255] },
      ],
    }
  )
}

export function GradientControls({
  typography,
  values = [typography],
  disabled,
  apply,
}: {
  typography: Typography
  values?: Typography[]
  disabled: boolean
  apply: ApplyTypography
}) {
  const { t } = useTranslation()
  const gradient = typography.fill_gradient
  const mixedMode = isMixed(values, (value) => !!value.fill_gradient)
  const update = (change: (value: FillGradient) => FillGradient) =>
    apply((value) => ({ ...value, fill_gradient: change(gradientFor(value)) }))
  return (
    <div className='grid min-w-0 gap-1.5'>
      <Select
        disabled={disabled}
        value={mixedMode ? '__mixed__' : gradient ? 'gradient' : 'color'}
        onValueChange={(mode) => {
          if (mode === 'gradient')
            apply((value) => ({ ...value, fill_gradient: gradientFor(value) }))
          if (mode === 'color') apply((value) => ({ ...value, fill_gradient: null }))
        }}
      >
        <SelectTrigger size='sm' aria-label={t('textExtras.fill')} className='w-full min-w-0'>
          <SelectValue>
            {t(
              mixedMode ? 'redesign.mixed' : gradient ? 'textExtras.gradient' : 'textExtras.color',
            )}
          </SelectValue>
        </SelectTrigger>
        <SelectContent>
          {mixedMode && (
            <SelectItem value='__mixed__' disabled>
              {t('redesign.mixed')}
            </SelectItem>
          )}
          <SelectItem value='color'>{t('textExtras.color')}</SelectItem>
          <SelectItem value='gradient'>{t('textExtras.gradient')}</SelectItem>
          <SelectItem value='screentone' disabled>
            {t('redesign.screentoneLater')}
          </SelectItem>
        </SelectContent>
      </Select>
      {gradient && !mixedMode ? (
        <>
          <div className='grid min-w-0 grid-cols-2 gap-1.5'>
            <Field label={t('textExtras.gradientKind')}>
              <SelectControl
                label={t('textExtras.gradientKind')}
                disabled={disabled}
                value={isMixed(values, (value) => value.fill_gradient?.kind) ? null : gradient.kind}
                options={[
                  ['Linear', t('textExtras.linear')],
                  ['Radial', t('textExtras.radial')],
                ]}
                onChange={(kind) => {
                  if (kind === 'Linear' || kind === 'Radial')
                    update((value) => ({ ...value, kind }))
                }}
              />
            </Field>
            {values.every((value) => value.fill_gradient?.kind === 'Linear') && (
              <Field label={t('textExtras.angle')}>
                <NumericControl
                  label={t('textExtras.angle')}
                  value={
                    isMixed(values, (value) => value.fill_gradient?.angle_degrees)
                      ? null
                      : gradient.angle_degrees
                  }
                  disabled={disabled}
                  min={-360}
                  max={360}
                  step={1}
                  onCommit={(angle_degrees) => update((value) => ({ ...value, angle_degrees }))}
                />
              </Field>
            )}
          </div>
          <div className='grid min-w-0 grid-cols-2 gap-1.5'>
            {(['start', 'end'] as const).map((end) => {
              const stop = end === 'start' ? gradient.stops[0] : gradient.stops.at(-1)
              const select = (value: Typography) =>
                end === 'start'
                  ? value.fill_gradient?.stops[0]?.color
                  : value.fill_gradient?.stops.at(-1)?.color
              return (
                <Field
                  key={end}
                  label={
                    isMixed(values, select) ? t('redesign.mixed') : t(`textExtras.${end}Color`)
                  }
                >
                  <ColorWell
                    label={t(`textExtras.${end}Color`)}
                    disabled={disabled}
                    size='sm'
                    value={rgbaToHex(stop?.color ?? [0, 0, 0, 255])}
                    onChange={(color) =>
                      update((value) => ({
                        ...value,
                        stops: value.stops.map((stop, index) =>
                          index === (end === 'start' ? 0 : value.stops.length - 1)
                            ? {
                                ...stop,
                                color: [...hexToRgba(color).slice(0, 3), stop.color[3]] as [
                                  number,
                                  number,
                                  number,
                                  number,
                                ],
                              }
                            : stop,
                        ),
                      }))
                    }
                  />
                </Field>
              )
            })}
          </div>
        </>
      ) : (
        <p className='text-[10px] text-muted-foreground'>
          {t(mixedMode ? 'redesign.mixedFillHint' : 'redesign.flatFillHint')}
        </p>
      )}
    </div>
  )
}
