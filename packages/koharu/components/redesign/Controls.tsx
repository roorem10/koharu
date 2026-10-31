'use client'

import { ArrowRightLeft, Minus, Plus } from 'lucide-react'
import { useEffect, useId, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { ColorWell } from '@/components/controls/ColorWell'
import type { ApplyTypography } from '@/components/text-extras/SpacingControls'
import { hexToRgba, rgbaToHex } from '@/lib/color'
import type { Typography } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import {
  NumberField,
  NumberFieldDecrement,
  NumberFieldGroup,
  NumberFieldIncrement,
  NumberFieldInput,
} from '@koharu/ui/components/number-field'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

import { defaultTypography, invertBorder } from './typography'
import { isMixed } from './useTypography'

export type TypographyControlsProps = {
  typography: Typography
  values: Typography[]
  disabled: boolean
  apply: ApplyTypography
}

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className='grid min-w-0 gap-0.5'>
      <span className='text-[8px] font-medium tracking-[0.06em] text-muted-foreground uppercase'>
        {label}
      </span>
      {children}
    </div>
  )
}

export function SelectControl({
  label,
  value,
  options,
  disabled,
  onChange,
}: {
  label: string
  value: string | null
  options: readonly (readonly [string, string])[]
  disabled?: boolean
  onChange: (value: string) => void
}) {
  const { t } = useTranslation()
  return (
    <Select
      disabled={disabled}
      value={value ?? '__mixed__'}
      onValueChange={(next) => {
        if (next !== null && next !== '__mixed__') onChange(next)
      }}
    >
      <SelectTrigger size='sm' aria-label={label} className='w-full min-w-0'>
        <SelectValue>
          {value === null ? t('redesign.mixed') : options.find(([id]) => id === value)?.[1]}
        </SelectValue>
      </SelectTrigger>
      <SelectContent>
        {value === null && (
          <SelectItem value='__mixed__' disabled>
            {t('redesign.mixed')}
          </SelectItem>
        )}
        {options.map(([id, name]) => (
          <SelectItem key={id} value={id}>
            {name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

export function NumericControl({
  label,
  value,
  disabled,
  min,
  max,
  step,
  onCommit,
  buttons = false,
  buttonLabels,
}: {
  label: string
  value: number | null
  disabled: boolean
  min: number
  max: number
  step: number
  onCommit: (value: number) => void
  buttons?: boolean
  buttonLabels?: [string, string]
}) {
  const { t } = useTranslation()
  const id = useId()
  const [draft, setDraft] = useState(value)
  useEffect(() => setDraft(value), [value])
  return (
    <NumberField
      id={id}
      value={draft}
      disabled={disabled}
      min={min}
      max={max}
      step={step}
      onValueChange={setDraft}
      onValueCommitted={(next) => {
        if (next !== null && Number.isFinite(next) && next >= min && next <= max) {
          if (next !== value) onCommit(next)
        } else setDraft(value)
      }}
    >
      <NumberFieldGroup>
        {buttons && (
          <NumberFieldDecrement
            aria-label={buttonLabels?.[0] ?? t('redesign.decrease', { field: label })}
          >
            <Minus />
          </NumberFieldDecrement>
        )}
        <NumberFieldInput
          aria-label={label}
          placeholder={value === null ? t('redesign.mixed') : undefined}
          onKeyDown={(event) => {
            if (event.key === 'Enter') event.currentTarget.blur()
          }}
        />
        {buttons && (
          <NumberFieldIncrement
            aria-label={buttonLabels?.[1] ?? t('redesign.increase', { field: label })}
          >
            <Plus />
          </NumberFieldIncrement>
        )}
      </NumberFieldGroup>
    </NumberField>
  )
}

export const outlineEnabled = (value: Typography) =>
  (value.stroke_width ?? 0) > 0 && (value.stroke_color ?? defaultTypography.stroke_color!)[3] > 0
export const outerEnabled = (value: Typography) =>
  outlineEnabled(value) &&
  (value.outer_stroke_width ?? 0) > 0 &&
  (value.outer_stroke_color ?? defaultTypography.outer_stroke_color!)[3] > 0

export function toggleOutline(value: Typography, enabled: boolean, outer = false): Typography {
  if (outer)
    return {
      ...value,
      outer_stroke_width: enabled
        ? (value.outer_stroke_width ?? 0) > 0
          ? value.outer_stroke_width
          : 3
        : 0,
      outer_stroke_color: enabled
        ? ([...(value.outer_stroke_color ?? [0, 0, 0, 255]).slice(0, 3), 255] as [
            number,
            number,
            number,
            number,
          ])
        : value.outer_stroke_color,
    }
  return {
    ...value,
    stroke_width: (value.stroke_width ?? 0) > 0 ? value.stroke_width : 1.5,
    stroke_color: [
      ...(value.stroke_color ?? [255, 255, 255, 255]).slice(0, 3),
      enabled ? 255 : 0,
    ] as [number, number, number, number],
  }
}

export function EffectCheckbox({
  label,
  checked,
  mixed,
  disabled,
  onChange,
  hideLabel = false,
}: {
  label: string
  checked: boolean
  mixed?: boolean
  disabled?: boolean
  onChange: (value: boolean) => void
  hideLabel?: boolean
}) {
  return (
    <label className='flex items-center gap-2 text-[11px]'>
      <Checkbox
        aria-label={label}
        checked={checked}
        indeterminate={mixed}
        disabled={disabled}
        onCheckedChange={(next) => onChange(mixed ? true : next)}
      />
      <span className={hideLabel ? 'sr-only' : undefined}>{label}</span>
    </label>
  )
}

export function OutlineControls({
  typography,
  values,
  disabled,
  apply,
  section = 'both',
}: TypographyControlsProps & { section?: 'outline' | 'outer' | 'both' }) {
  const { t } = useTranslation()
  const mixed = (key: keyof Typography) => isMixed(values, (value) => value[key])
  const strokeEnabled = outlineEnabled(typography)
  const method = typography.stroke_method ?? 'contour'
  const primary = section !== 'outer'
  const outer = section !== 'outline'
  const strokeColor = typography.stroke_color ?? defaultTypography.stroke_color!
  const outerColor = typography.outer_stroke_color ?? defaultTypography.outer_stroke_color!
  return (
    <div className='grid min-w-0 gap-2'>
      {primary && (
        <>
          <EffectCheckbox
            label={t('redesign.outlineOn')}
            checked={strokeEnabled}
            mixed={isMixed(values, outlineEnabled)}
            disabled={disabled}
            onChange={(enabled) => apply((value) => toggleOutline(value, enabled))}
          />
          <div className='grid grid-cols-[3rem_minmax(0,1fr)_1.5rem] items-end gap-1.5'>
            <Field label={mixed('stroke_color') ? t('redesign.mixed') : t('inspector.border')}>
              <ColorWell
                label={t('inspector.borderColor')}
                size='sm'
                disabled={disabled}
                allowTransparent
                value={strokeEnabled ? rgbaToHex(strokeColor) : null}
                onChange={(color) =>
                  apply((value) =>
                    color
                      ? { ...toggleOutline(value, true), stroke_color: hexToRgba(color) }
                      : toggleOutline(value, false),
                  )
                }
              />
            </Field>
            <Field label={t('inspector.width')}>
              <NumericControl
                label={t('inspector.borderWidth')}
                disabled={disabled}
                min={0.5}
                max={32}
                step={0.5}
                value={
                  mixed('stroke_width')
                    ? null
                    : (typography.stroke_width ?? 0) > 0
                      ? typography.stroke_width!
                      : 1.5
                }
                buttons
                buttonLabels={[
                  t('inspector.decreaseBorderWidth'),
                  t('inspector.increaseBorderWidth'),
                ]}
                onCommit={(stroke_width) => apply((value) => ({ ...value, stroke_width }))}
              />
            </Field>
            <Button
              type='button'
              variant='ghost'
              size='icon-xs'
              aria-label={t('inspector.swapFillOutline')}
              title={t('inspector.swapFillOutline')}
              disabled={disabled}
              onClick={() =>
                apply((value) =>
                  invertBorder(
                    value,
                    (typography.stroke_width ?? 0) > 0 ? typography.stroke_width! : 1.5,
                  ),
                )
              }
            >
              <ArrowRightLeft className='size-3' />
            </Button>
          </div>
          <div className='grid min-w-0 grid-cols-2 gap-1.5'>
            <Field label={t('inspector.outlineMethod')}>
              <SelectControl
                label={t('inspector.outlineMethod')}
                disabled={disabled}
                value={mixed('stroke_method') ? null : method}
                options={[
                  ['expansion', t('inspector.outlineExpansion')],
                  ['contour', t('inspector.outlineContour')],
                ]}
                onChange={(stroke_method) =>
                  apply((value) => ({
                    ...value,
                    stroke_method: stroke_method as Typography['stroke_method'],
                  }))
                }
              />
            </Field>
            <Field label={t('inspector.outlinePosition')}>
              <SelectControl
                label={t('inspector.outlinePosition')}
                disabled={disabled || method !== 'contour' || mixed('stroke_method')}
                value={mixed('stroke_position') ? null : (typography.stroke_position ?? 'outside')}
                options={(['outside', 'center', 'inside'] as const).map((id) => [
                  id,
                  t(`inspector.outline${id[0].toUpperCase() + id.slice(1)}`),
                ])}
                onChange={(stroke_position) =>
                  apply((value) => ({
                    ...value,
                    stroke_position: stroke_position as Typography['stroke_position'],
                  }))
                }
              />
            </Field>
            <Field label={t('inspector.outlineJoin')}>
              <SelectControl
                label={t('inspector.outlineJoin')}
                disabled={disabled || method !== 'contour' || mixed('stroke_method')}
                value={mixed('stroke_join') ? null : (typography.stroke_join ?? 'round')}
                options={(['round', 'miter', 'bevel'] as const).map((id) => [
                  id,
                  t(`inspector.outline${id[0].toUpperCase() + id.slice(1)}`),
                ])}
                onChange={(stroke_join) =>
                  apply((value) => ({
                    ...value,
                    stroke_join: stroke_join as Typography['stroke_join'],
                  }))
                }
              />
            </Field>
          </div>
        </>
      )}
      {outer && (
        <>
          <EffectCheckbox
            label={t('redesign.secondOutlineOn')}
            checked={outerEnabled(typography)}
            mixed={isMixed(values, outerEnabled)}
            disabled={disabled || values.some((value) => !outlineEnabled(value))}
            onChange={(enabled) => apply((value) => toggleOutline(value, enabled, true))}
          />
          <div className='grid grid-cols-[3rem_minmax(0,1fr)] items-end gap-1.5'>
            <Field
              label={
                mixed('outer_stroke_color') ? t('redesign.mixed') : t('inspector.secondBorder')
              }
            >
              <ColorWell
                label={t('inspector.secondBorderColor')}
                size='sm'
                disabled={disabled || values.some((value) => !outlineEnabled(value))}
                allowTransparent
                value={outerEnabled(typography) ? rgbaToHex(outerColor) : null}
                onChange={(color) =>
                  apply((value) =>
                    color
                      ? {
                          ...toggleOutline(value, true, true),
                          outer_stroke_color: hexToRgba(color),
                        }
                      : toggleOutline(value, false, true),
                  )
                }
              />
            </Field>
            <Field label={t('inspector.width')}>
              <NumericControl
                label={t('inspector.secondBorderWidth')}
                disabled={disabled || values.some((value) => !outlineEnabled(value))}
                min={0.5}
                max={32}
                step={0.5}
                value={
                  mixed('outer_stroke_width')
                    ? null
                    : (typography.outer_stroke_width ?? 0) > 0
                      ? typography.outer_stroke_width!
                      : 3
                }
                buttons
                buttonLabels={[
                  t('inspector.decreaseSecondBorderWidth'),
                  t('inspector.increaseSecondBorderWidth'),
                ]}
                onCommit={(outer_stroke_width) =>
                  apply((value) => ({
                    ...value,
                    outer_stroke_width,
                    outer_stroke_color: value.outer_stroke_color ?? [0, 0, 0, 255],
                  }))
                }
              />
            </Field>
          </div>
        </>
      )}
    </div>
  )
}
