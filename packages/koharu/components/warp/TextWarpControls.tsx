'use client'

import { useEffect, useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useCompareStore } from '@/components/qc/compare'
import type { EntityId } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { NumberField, NumberFieldGroup, NumberFieldInput } from '@koharu/ui/components/number-field'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

import { unitCorners } from './geometry'
import { useTextWarp } from './useTextWarp'

export function TextWarpControls({ layers }: { layers: EntityId[] }) {
  const { t } = useTranslation()
  const { warps, ready, busy, failed, setWarp } = useTextWarp(layers)
  const showingOriginal = useCompareStore((state) => state.pinned || state.held)
  const first = warps?.[0] ?? null
  const mixed = warps?.some((warp) => JSON.stringify(warp) !== JSON.stringify(first)) ?? false
  const sameMode = warps?.every((warp) => warp?.kind === first?.kind) ?? true
  const mode = sameMode ? (first?.kind ?? 'none') : 'mixed'
  const disabled = !ready || busy || showingOriginal
  return (
    <div className='grid min-w-0 gap-2'>
      <Select
        disabled={disabled}
        value={mode === 'mixed' ? null : mode}
        onValueChange={(value) => {
          if (disabled || value === mode) return
          if (value === 'none') setWarp(null)
          if (value === 'skew') setWarp({ kind: 'skew', x_degrees: 0, y_degrees: 0 })
          if (value === 'perspective') setWarp({ kind: 'perspective', corners: unitCorners() })
        }}
      >
        <SelectTrigger size='sm' aria-label={t('warp.mode')} className='w-full min-w-0'>
          <SelectValue>{t(`warp.${mode}`)}</SelectValue>
        </SelectTrigger>
        <SelectContent>
          <SelectItem value='none'>{t('warp.none')}</SelectItem>
          <SelectItem value='skew'>{t('warp.skew')}</SelectItem>
          <SelectItem value='perspective'>{t('warp.perspective')}</SelectItem>
        </SelectContent>
      </Select>
      {mixed && <p className='text-[10px] text-muted-foreground'>{t('warp.multiple')}</p>}
      {mode === 'skew' && first?.kind === 'skew' && (
        <>
          <div className='grid min-w-0 grid-cols-2 gap-2'>
            {(['x_degrees', 'y_degrees'] as const).map((axis) => (
              <AngleField
                key={JSON.stringify([layers, axis])}
                label={t(axis === 'x_degrees' ? 'warp.horizontal' : 'warp.vertical')}
                value={first[axis]}
                mixed={Boolean(
                  warps?.some((warp) => warp?.kind !== 'skew' || warp[axis] !== first[axis]),
                )}
                disabled={disabled}
                onCommit={(value) => setWarp({ ...first, [axis]: value })}
              />
            ))}
          </div>
          <Button
            variant='outline'
            size='sm'
            disabled={disabled}
            onClick={() => setWarp({ kind: 'skew', x_degrees: 0, y_degrees: 0 })}
          >
            {t('warp.resetSkew')}
          </Button>
        </>
      )}
      {mode === 'perspective' && (
        <>
          <Button
            variant='outline'
            size='sm'
            disabled={disabled}
            onClick={() => setWarp({ kind: 'perspective', corners: unitCorners() })}
          >
            {t('warp.resetCorners')}
          </Button>
          <p className='text-[10px] text-muted-foreground'>{t('warp.dragHint')}</p>
        </>
      )}
      {failed && (
        <p role='alert' className='text-[10px]'>
          {t('warp.error')}
        </p>
      )}
    </div>
  )
}

function AngleField({
  label,
  value,
  mixed,
  disabled,
  onCommit,
}: {
  label: string
  value: number
  mixed: boolean
  disabled: boolean
  onCommit: (value: number) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const [draft, setDraft] = useState<number | null>(mixed ? null : value)
  useEffect(() => setDraft(mixed ? null : value), [value, mixed])
  return (
    <div className='grid min-w-0 gap-1'>
      <label htmlFor={id} className='text-[10px] text-muted-foreground'>
        {label}
      </label>
      <NumberField
        id={id}
        value={draft}
        disabled={disabled}
        min={-60}
        max={60}
        step={1}
        onValueChange={setDraft}
        onValueCommitted={(next) => {
          if (
            next !== null &&
            Number.isFinite(next) &&
            next >= -60 &&
            next <= 60 &&
            (next !== value || mixed)
          )
            onCommit(next)
          else setDraft(mixed ? null : value)
        }}
      >
        <NumberFieldGroup>
          <NumberFieldInput
            aria-label={label}
            placeholder={mixed ? t('warp.mixed') : undefined}
            onKeyDown={(event) => {
              if (event.key === 'Enter') event.currentTarget.blur()
            }}
          />
        </NumberFieldGroup>
      </NumberField>
    </div>
  )
}
