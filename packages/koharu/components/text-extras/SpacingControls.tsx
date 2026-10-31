'use client'

import { useEffect, useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { Typography } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { NumberField, NumberFieldGroup, NumberFieldInput } from '@koharu/ui/components/number-field'

export type ApplyTypography = (update: (value: Typography) => Typography) => void

export function SpacingControls({
  typography,
  disabled,
  apply,
}: {
  typography: Typography
  disabled: boolean
  apply: ApplyTypography
}) {
  const { t } = useTranslation()
  return (
    <div className='grid min-w-0 grid-cols-2 gap-1.5'>
      <SpacingField
        label={t('textExtras.lineSpacing')}
        resetLabel={t('textExtras.resetLineSpacing')}
        disabled={disabled}
        value={typography.line_spacing ?? 1.2}
        min={0.5}
        max={3}
        step={0.05}
        onCommit={(line_spacing) => apply((value) => ({ ...value, line_spacing }))}
      />
      <SpacingField
        label={t('textExtras.letterSpacing')}
        resetLabel={t('textExtras.resetLetterSpacing')}
        disabled={disabled}
        value={typography.letter_spacing ?? 0}
        min={-500}
        max={2000}
        step={10}
        onCommit={(letter_spacing) => apply((value) => ({ ...value, letter_spacing }))}
      />
    </div>
  )
}

function SpacingField({
  label,
  resetLabel,
  value,
  min,
  max,
  step,
  disabled,
  onCommit,
}: {
  label: string
  resetLabel: string
  value: number
  min: number
  max: number
  step: number
  disabled: boolean
  onCommit: (value: number | null) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const [draft, setDraft] = useState<number | null>(value)
  useEffect(() => setDraft(value), [value])
  return (
    <div className='grid min-w-0 gap-0.5'>
      <label htmlFor={id} className='text-[8px] font-medium text-muted-foreground'>
        {label}
      </label>
      <div className='flex min-w-0 gap-0.5'>
        <NumberField
          id={id}
          className='min-w-0 flex-1'
          value={draft}
          min={min}
          max={max}
          step={step}
          disabled={disabled}
          onValueChange={setDraft}
          onValueCommitted={(next) => {
            if (next !== null && Number.isFinite(next) && next >= min && next <= max) {
              if (next !== value) onCommit(next)
            } else setDraft(value)
          }}
        >
          <NumberFieldGroup>
            <NumberFieldInput
              aria-label={label}
              onKeyDown={(event) => {
                if (event.key === 'Enter') event.currentTarget.blur()
              }}
            />
          </NumberFieldGroup>
        </NumberField>
        <Button
          type='button'
          size='sm'
          variant='ghost'
          className='h-6 shrink-0 px-1 text-[9px]'
          aria-label={resetLabel}
          disabled={disabled}
          onClick={() => onCommit(null)}
        >
          {t('textExtras.reset')}
        </Button>
      </div>
    </div>
  )
}
