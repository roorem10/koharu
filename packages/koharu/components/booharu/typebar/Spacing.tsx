'use client'

import { LineHeight, RotateCcw } from 'lucide-react'
import { useEffect, useId, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { WholeBoxOnly } from '@/components/booharu/typebar/AppliesTo'
import { DEFAULT_LINE_HEIGHT } from '@/lib/booharu/mixed'
import type { WordRange } from '@/lib/booharu/words'
import { useKoharuStore } from '@/lib/store'
import { NumberField, NumberFieldGroup, NumberFieldInput } from '@koharu/ui/components/number-field'
import { Popover, PopoverContent, PopoverTrigger } from '@koharu/ui/components/popover'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** Line spacing's range and step: a multiple of each line's largest size. */
const LINE = { min: 0.5, max: 3, step: 0.05 }

/** Letter spacing's range and step, in thousandths of an em, as Photoshop's tracking. */
const LETTER = { min: -500, max: 2000, step: 10 }

/** What the type bar hands Spacing: the values its targets share, and its edits. */
export interface SpacingProps {
  /** The targets' shared line spacing, `null` where none sets one. */
  lineHeight: number | null
  /** Whether the targets' line spacings differ. */
  lineHeightMixed: boolean
  /** Whether there's a paragraph for line spacing: a new text being typed has none yet. */
  paragraph: boolean
  /** The letter spacing the characters share, `null` where none sets one. */
  tracking: number | null
  /** Whether the characters' letter spacings differ. */
  trackingMixed: boolean
  /** The words chosen in the Story panel that the bar's character settings apply to. */
  words?: WordRange | null
  /** Sets every target's line spacing to `lineHeight`, or takes it off with `null`. */
  onLineHeight: (lineHeight: number | null) => void
  /** Gives the characters the bar styles the letter spacing `tracking`, at its scope: the
   * characters typed, the words chosen, or every target's; `null` takes it off whole boxes. */
  onTracking: (tracking: number | null) => void
}

/** Booharu's spacing in the type bar (S12-k): a button after the alignment group, whose popover
 * holds the paragraph's line spacing, as S11-i made it, and the characters' letter spacing, each a
 * number box with its Reset. Line spacing goes from 0.5 to 3.0 in steps of 0.05 and shows the
 * targets' shared spacing, Koharu's 1.2 where none sets one; a new text being typed has no
 * paragraph yet, so it's off then, and while the canvas types, or with words chosen in Story, its
 * label says it's the whole box's (S11-f). Letter spacing, in thousandths of an em, goes from -500
 * to 2000 in steps of 10, shows 0 where none is set, and goes to the bar's scope, as its other
 * character settings do. Either shows nothing, "Mixed", where the values differ. A typed value
 * goes on Enter or as the box loses the focus, a step of the arrow keys or of a turn of the mouse
 * wheel at once. Reset takes line spacing off, back to Koharu's 1.2, and letter spacing off whole
 * boxes; characters typed or words chosen get 0, as a mark can't unset a value. Spacing isn't a
 * text effect, as 0.0.4 had it, so letter spacing stays out of Text effects. */
export function Spacing({
  lineHeight,
  lineHeightMixed,
  paragraph,
  tracking,
  trackingMixed,
  words = null,
  onLineHeight,
  onTracking,
}: SpacingProps) {
  const { t } = useTranslation()
  const typing = useKoharuStore((state) => state.typing !== null)
  // Whole boxes take letter spacing in their styles, which a null takes off.
  const wholeBoxes = !typing && !words
  return (
    <Popover>
      <Tooltip>
        <TooltipTrigger
          render={
            <PopoverTrigger
              aria-label={t('booharu.typebar.spacing')}
              className='grid size-6 shrink-0 place-items-center rounded-md border border-input bg-background text-muted-foreground hover:bg-muted'
            />
          }
        >
          <LineHeight aria-hidden='true' className='size-3.5' />
        </TooltipTrigger>
        <TooltipContent>{t('booharu.typebar.spacing')}</TooltipContent>
      </Tooltip>
      <PopoverContent side='bottom' align='center' className='w-56 gap-2 rounded-xl p-2'>
        <SpacingField
          name='line-spacing'
          label={t('booharu.typebar.lineSpacing')}
          note={<WholeBoxOnly words={words} />}
          resetLabel={t('booharu.typebar.lineSpacingReset')}
          value={lineHeightMixed ? null : (lineHeight ?? DEFAULT_LINE_HEIGHT)}
          mixed={lineHeightMixed}
          disabled={!paragraph}
          range={LINE}
          onChange={onLineHeight}
          onReset={() => onLineHeight(null)}
        />
        <SpacingField
          name='letter-spacing'
          label={t('booharu.typebar.letterSpacing')}
          resetLabel={t('booharu.typebar.letterSpacingReset')}
          value={trackingMixed ? null : (tracking ?? 0)}
          mixed={trackingMixed}
          range={LETTER}
          onChange={onTracking}
          onReset={() => onTracking(wholeBoxes ? null : 0)}
        />
        <p className='text-[10px] leading-4 text-muted-foreground'>
          {t('booharu.typebar.wheelHint')}
        </p>
      </PopoverContent>
    </Popover>
  )
}

/** A spacing's number box under its label, with its Reset: `value` shown, nothing where it's
 * `null`, with "Mixed" when `mixed`. A value goes once: Enter sends it, and the box's commit as it
 * loses the focus doesn't send it again, nor a value the box shows. */
function SpacingField({
  name,
  label,
  note,
  resetLabel,
  value,
  mixed,
  disabled = false,
  range,
  onChange,
  onReset,
}: {
  name: string
  label: string
  /** What the label adds, such as whose the value is. */
  note?: ReactNode
  resetLabel: string
  value: number | null
  mixed: boolean
  disabled?: boolean
  range: { min: number; max: number; step: number }
  onChange: (value: number) => void
  onReset: () => void
}) {
  const { t } = useTranslation()
  const id = useId()
  // What the box shows while a value is typed, as the type bar's size field holds it.
  const [draft, setDraft] = useState<number | null>(value)
  // The value the box last showed or sent, none after a Reset. Words chosen and characters typed
  // show no draft, so `value` shows a value sent only once the targets take it.
  const last = useRef<number | null | undefined>(value)

  useEffect(() => {
    setDraft(value)
    last.current = value
  }, [value])

  const commit = (next: number | null) => {
    if (next === null || next < range.min || next > range.max) return setDraft(value)
    if (next === last.current) return
    last.current = next
    onChange(next)
  }

  const reset = () => {
    last.current = undefined
    onReset()
  }

  return (
    <div className='flex flex-col gap-1'>
      <span className='text-[10px] font-medium text-muted-foreground'>
        {label}
        {note}
      </span>
      <div className='flex items-center gap-1'>
        <NumberField
          id={id}
          name={name}
          className='min-w-0 flex-1'
          value={draft}
          min={range.min}
          max={range.max}
          step={range.step}
          disabled={disabled}
          onValueChange={setDraft}
          onValueCommitted={commit}
        >
          <NumberFieldGroup>
            <NumberFieldInput
              aria-label={label}
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
                aria-label={resetLabel}
                disabled={disabled}
                className='grid size-6 shrink-0 place-items-center rounded-md border border-input bg-background text-muted-foreground hover:bg-muted disabled:pointer-events-none disabled:opacity-50'
                onClick={reset}
              />
            }
          >
            <RotateCcw aria-hidden='true' className='size-3.5' />
          </TooltipTrigger>
          <TooltipContent>{resetLabel}</TooltipContent>
        </Tooltip>
      </div>
    </div>
  )
}
