'use client'

import type { ParseKeys } from 'i18next'
import { Minus, Plus } from 'lucide-react'
import { useId } from 'react'
import { useTranslation } from 'react-i18next'

import { isBrushTool, setBrushSlot, useBrushSlots, type DrawSlot } from '@/lib/booharu/brush'
import { useKoharuStore } from '@/lib/store'
import {
  NumberField,
  NumberFieldDecrement,
  NumberFieldGroup,
  NumberFieldIncrement,
  NumberFieldInput,
} from '@koharu/ui/components/number-field'
import { Slider } from '@koharu/ui/components/slider'

/** The brush tool's own options, under its size in the brush size popover: its hardness, for
 * Brush, the Eraser and Remove; Draw's opacity; and, for Remove, where its clean-up goes. Each is
 * a percentage, kept in the tool's brush from 0 to 1. Nothing for another tool. */
export function BrushOptions() {
  const { t } = useTranslation()
  const tool = useKoharuStore((state) => state.tool)
  const slots = useBrushSlots()
  if (!isBrushTool(tool)) return null
  const slot: Partial<DrawSlot> = slots[tool]

  return (
    <>
      <PercentRow
        label='booharu.cleanup.hardness'
        less='booharu.cleanup.decreaseHardness'
        more='booharu.cleanup.increaseHardness'
        value={slot.hardness ?? 1}
        min={0}
        onChange={(hardness) => setBrushSlot(tool, { hardness })}
      />
      {tool === 'draw' && (
        <PercentRow
          label='booharu.cleanup.opacity'
          less='booharu.cleanup.decreaseOpacity'
          more='booharu.cleanup.increaseOpacity'
          value={slot.opacity ?? 1}
          min={1}
          onChange={(opacity) => setBrushSlot(tool, { opacity })}
        />
      )}
      {tool === 'remove' && (
        <p className='text-[10px] leading-4 text-muted-foreground'>
          {t('booharu.cleanup.removeNote')}
        </p>
      )}
    </>
  )
}

/** A share from `min` percent to all of it, as the size row above it lays it out: its label and a
 * number box on one line, in whole percent, and a slider under them. `value` and what `onChange`
 * hears run from 0 to 1. */
function PercentRow({
  label,
  less,
  more,
  value,
  min,
  onChange,
}: {
  label: ParseKeys
  less: ParseKeys
  more: ParseKeys
  value: number
  min: number
  onChange: (value: number) => void
}) {
  const { t } = useTranslation()
  const labelId = useId()
  const percent = Math.round(value * 100)
  const change = (next: number | null) => {
    if (next !== null && Number.isFinite(next)) onChange(clamp(Math.round(next), min, 100) / 100)
  }

  return (
    <>
      <div className='flex items-center justify-between gap-3'>
        <span id={labelId} className='text-[11px] font-medium'>
          {t(label)}
        </span>
        <NumberField
          min={min}
          max={100}
          step={1}
          value={percent}
          format={{ style: 'unit', unit: 'percent', maximumFractionDigits: 0 }}
          className='w-20'
          onValueChange={change}
        >
          <NumberFieldGroup className='h-7'>
            <NumberFieldDecrement aria-label={t(less)}>
              <Minus />
            </NumberFieldDecrement>
            <NumberFieldInput aria-labelledby={labelId} />
            <NumberFieldIncrement aria-label={t(more)}>
              <Plus />
            </NumberFieldIncrement>
          </NumberFieldGroup>
        </NumberField>
      </div>
      <Slider
        aria-labelledby={labelId}
        min={min}
        max={100}
        step={1}
        value={percent}
        className='py-1 [&_[data-slot=slider-thumb]]:size-2.5'
        onValueChange={(next) => change(typeof next === 'number' ? next : (next[0] ?? null))}
      />
    </>
  )
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}
