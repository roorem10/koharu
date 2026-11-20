'use client'

import { LineHeight, RotateCcw } from 'lucide-react'
import { useEffect, useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { DEFAULT_LINE_HEIGHT } from '@/lib/booharu/mixed'
import { NumberField, NumberFieldGroup, NumberFieldInput } from '@koharu/ui/components/number-field'
import { Popover, PopoverContent, PopoverTrigger } from '@koharu/ui/components/popover'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

const MIN = 0.5
const MAX = 3

/** Booharu's line spacing in the type bar (S11-i): a button after the alignment group, whose
 * popover holds a number box from 0.5 to 3.0 in steps of 0.05 and Reset. The box shows the
 * targets' shared spacing, Koharu's 1.2 where none sets one, or nothing, "Mixed", where they
 * differ. A typed value goes on Enter or as the box loses the focus, a step of the arrow keys or
 * of a turn of the mouse wheel at once, each one change of the line spacing alone; Reset takes it
 * off, back to Koharu's 1.2. A new text being typed has no paragraph yet, so it's disabled. */
export function LineSpacing({
  value,
  mixed,
  disabled,
  onChange,
}: {
  /** The targets' shared spacing, `null` where none sets one. */
  value: number | null
  /** Whether the targets' spacings differ. */
  mixed: boolean
  disabled: boolean
  /** Sets every target's spacing to `lineHeight`, or takes it off with `null`. */
  onChange: (lineHeight: number | null) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const shown = mixed ? null : (value ?? DEFAULT_LINE_HEIGHT)
  // What the box shows while a value is typed, as the type bar's size field holds it.
  const [draft, setDraft] = useState<number | null>(shown)

  useEffect(() => setDraft(shown), [shown])

  const commit = (next: number | null) => {
    if (next === null || next < MIN || next > MAX) return setDraft(shown)
    if (next !== shown) onChange(next)
  }

  return (
    <Popover>
      <Tooltip>
        <TooltipTrigger
          render={
            <PopoverTrigger
              aria-label={t('booharu.typebar.lineSpacing')}
              disabled={disabled}
              className='grid size-6 shrink-0 place-items-center rounded-md border border-input bg-background text-muted-foreground hover:bg-muted disabled:pointer-events-none disabled:opacity-50'
            />
          }
        >
          <LineHeight aria-hidden='true' className='size-3.5' />
        </TooltipTrigger>
        <TooltipContent>{t('booharu.typebar.lineSpacing')}</TooltipContent>
      </Tooltip>
      <PopoverContent side='bottom' align='center' className='w-56 gap-1.5 rounded-xl p-2'>
        <span className='text-[10px] font-medium text-muted-foreground'>
          {t('booharu.typebar.lineSpacing')}
        </span>
        <div className='flex items-center gap-1'>
          <NumberField
            id={id}
            name='line-spacing'
            className='min-w-0 flex-1'
            value={draft}
            min={MIN}
            max={MAX}
            step={0.05}
            onValueChange={setDraft}
            onValueCommitted={commit}
          >
            <NumberFieldGroup>
              <NumberFieldInput
                aria-label={t('booharu.typebar.lineSpacing')}
                placeholder={mixed ? t('booharu.typebar.mixed') : undefined}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') commit(draft)
                }}
              />
            </NumberFieldGroup>
          </NumberField>
          <Tooltip>
            <TooltipTrigger
              render={
                <button
                  type='button'
                  aria-label={t('booharu.typebar.lineSpacingReset')}
                  className='grid size-6 shrink-0 place-items-center rounded-md border border-input bg-background text-muted-foreground hover:bg-muted'
                  onClick={() => onChange(null)}
                />
              }
            >
              <RotateCcw aria-hidden='true' className='size-3.5' />
            </TooltipTrigger>
            <TooltipContent>{t('booharu.typebar.lineSpacingReset')}</TooltipContent>
          </Tooltip>
        </div>
        <p className='text-[10px] leading-4 text-muted-foreground'>
          {t('booharu.typebar.wheelHint')}
        </p>
      </PopoverContent>
    </Popover>
  )
}
