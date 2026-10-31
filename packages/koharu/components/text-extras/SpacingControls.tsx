'use client'

import { useTranslation } from 'react-i18next'

import { Field, NumericControl } from '@/components/redesign/Controls'
import { isMixed } from '@/components/redesign/useTypography'
import type { Typography } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

export type ApplyTypography = (update: (value: Typography) => Typography) => void

export function SpacingControls({
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
  return (
    <div className='grid min-w-0 grid-cols-2 gap-1.5'>
      {(['line_spacing', 'letter_spacing'] as const).map((key) => {
        const line = key === 'line_spacing'
        const label = t(line ? 'textExtras.lineSpacing' : 'textExtras.letterSpacing')
        return (
          <Field key={key} label={label}>
            <div className='flex min-w-0 gap-0.5'>
              <div className='min-w-0 flex-1'>
                <NumericControl
                  label={label}
                  disabled={disabled}
                  value={
                    isMixed(values, (value) => value[key])
                      ? null
                      : (typography[key] ?? (line ? 1.2 : 0))
                  }
                  min={line ? 0.5 : -500}
                  max={line ? 3 : 2000}
                  step={line ? 0.05 : 10}
                  onCommit={(next) => apply((value) => ({ ...value, [key]: next }))}
                />
              </div>
              <Button
                type='button'
                size='sm'
                variant='ghost'
                className='h-6 shrink-0 px-1 text-[9px]'
                aria-label={t(
                  line ? 'textExtras.resetLineSpacing' : 'textExtras.resetLetterSpacing',
                )}
                disabled={disabled}
                onClick={() => apply((value) => ({ ...value, [key]: null }))}
              >
                {t('textExtras.reset')}
              </Button>
            </div>
          </Field>
        )
      })}
    </div>
  )
}
