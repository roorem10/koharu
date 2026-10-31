'use client'

import { useEffect, useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { ColorWell } from '@/components/controls/ColorWell'
import { hexToRgba, rgbaToHex } from '@/lib/color'
import type { FillGradient, Typography } from '@koharu/bridge/protocol'
import { NumberField, NumberFieldGroup, NumberFieldInput } from '@koharu/ui/components/number-field'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

import type { ApplyTypography } from './SpacingControls'

function gradientFor(value: Typography): FillGradient {
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
  disabled,
  apply,
}: {
  typography: Typography
  disabled: boolean
  apply: ApplyTypography
}) {
  const { t } = useTranslation()
  const angleId = useId()
  const gradient = typography.fill_gradient
  const [angle, setAngle] = useState<number | null>(gradient?.angle_degrees ?? 0)
  useEffect(() => setAngle(gradient?.angle_degrees ?? 0), [gradient?.angle_degrees])
  const update = (change: (value: FillGradient) => FillGradient) =>
    apply((value) => ({ ...value, fill_gradient: change(gradientFor(value)) }))
  return (
    <div className='grid min-w-0 gap-1.5'>
      <Select
        disabled={disabled}
        value={gradient ? 'gradient' : 'color'}
        onValueChange={(mode) => {
          if (mode === 'gradient')
            apply((value) => ({ ...value, fill_gradient: gradientFor(value) }))
          if (mode === 'color') apply((value) => ({ ...value, fill_gradient: null }))
        }}
      >
        <SelectTrigger size='sm' aria-label={t('textExtras.fill')} className='w-full min-w-0'>
          <SelectValue>{t(gradient ? 'textExtras.gradient' : 'textExtras.color')}</SelectValue>
        </SelectTrigger>
        <SelectContent>
          <SelectItem value='color'>{t('textExtras.color')}</SelectItem>
          <SelectItem value='gradient'>{t('textExtras.gradient')}</SelectItem>
        </SelectContent>
      </Select>
      {gradient && (
        <>
          <div className='grid min-w-0 grid-cols-2 gap-1.5'>
            <Select
              disabled={disabled}
              value={gradient.kind}
              onValueChange={(kind) => {
                if (kind === 'Linear' || kind === 'Radial') update((value) => ({ ...value, kind }))
              }}
            >
              <SelectTrigger
                size='sm'
                aria-label={t('textExtras.gradientKind')}
                className='w-full min-w-0'
              >
                <SelectValue>
                  {t(gradient.kind === 'Linear' ? 'textExtras.linear' : 'textExtras.radial')}
                </SelectValue>
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='Linear'>{t('textExtras.linear')}</SelectItem>
                <SelectItem value='Radial'>{t('textExtras.radial')}</SelectItem>
              </SelectContent>
            </Select>
            {gradient.kind === 'Linear' && (
              <NumberField
                id={angleId}
                value={angle}
                min={-360}
                max={360}
                step={1}
                disabled={disabled}
                onValueChange={setAngle}
                onValueCommitted={(next) => {
                  if (next !== null && Number.isFinite(next) && next !== gradient.angle_degrees)
                    update((value) => ({ ...value, angle_degrees: next }))
                  else setAngle(gradient.angle_degrees)
                }}
              >
                <NumberFieldGroup>
                  <NumberFieldInput
                    aria-label={t('textExtras.angle')}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') event.currentTarget.blur()
                    }}
                  />
                </NumberFieldGroup>
              </NumberField>
            )}
          </div>
          <div className='flex min-w-0 flex-wrap gap-3'>
            {(['start', 'end'] as const).map((end) => {
              const stop = gradient.stops[end === 'start' ? 0 : gradient.stops.length - 1]
              return (
                <div key={end} className='flex min-w-0 items-center gap-1'>
                  <ColorWell
                    size='sm'
                    disabled={disabled}
                    label={t(`textExtras.${end}Color`)}
                    value={rgbaToHex(stop.color)}
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
                  <span className='min-w-0 text-[9px] text-muted-foreground'>
                    {t(`textExtras.${end}Color`)}
                  </span>
                </div>
              )
            })}
          </div>
        </>
      )}
    </div>
  )
}
