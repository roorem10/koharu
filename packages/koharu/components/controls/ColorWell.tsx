'use client'

import { Pipette, SquareSlash } from 'lucide-react'
import { useEffect, useState } from 'react'
import { HexColorInput, HexColorPicker } from 'react-colorful'
import { useTranslation } from 'react-i18next'

import { useColorSampling } from '@/components/controls/ColorSampling'
import { Button } from '@koharu/ui/components/button'
import { Popover, PopoverContent, PopoverTrigger } from '@koharu/ui/components/popover'
import { cn } from '@koharu/ui/lib/utils'

type ColorWellProps = {
  label?: string
  size?: 'default' | 'sm'
  emptyLabel?: string
  disabled?: boolean
} & (
  | {
      value: string
      allowTransparent?: false
      onChange: (color: string) => void
    }
  | {
      value: string | null
      allowTransparent: true
      onChange: (color: string | null) => void
    }
)

export function ColorWell(props: ColorWellProps) {
  const { t } = useTranslation()
  const { value, label, size = 'default', disabled = false, allowTransparent = false } = props
  const [draft, setDraft] = useState(value ?? '#000000')
  const [transparent, setTransparent] = useState(value === null)
  // A picker drag can update the swatch without its end event firing (for example when the
  // drag ends in the black corner). Such a dragged-but-uncommitted color is kept here, apart
  // from the committed `value`, so closing the picker can apply it and Escape can discard it.
  // Every other way of settling the color (a committed drag, a typed hex code, Transparent, the
  // eyedropper or a new value from outside) clears it, so it can never override a later choice.
  const [pending, setPending] = useState<string | null>(null)
  const [open, setOpen] = useState(false)
  const sampling = useColorSampling()
  const accessibleLabel = label ?? t('colorPicker.brushColor')

  useEffect(() => {
    setPending(null)
    if (value === null) {
      setTransparent(true)
    } else {
      setDraft(value)
      setTransparent(false)
    }
  }, [value])

  const set = (color: string) => {
    const normalized = normalize(color)
    setDraft(normalized)
    setPending(null)
    setTransparent(false)
    if (props.allowTransparent) props.onChange(normalized)
    else props.onChange(normalized)
  }

  const clear = () => {
    if (!props.allowTransparent) return
    setPending(null)
    setTransparent(true)
    props.onChange(null)
  }

  return (
    <Popover
      open={open}
      onOpenChange={(next, details) => {
        if (!next && pending !== null) {
          if (details.reason === 'escape-key') {
            // Escape cancels: show the committed color again and apply nothing.
            setPending(null)
            if (value !== null) setDraft(value)
          } else {
            set(pending)
          }
        }
        setOpen(next)
      }}
    >
      <PopoverTrigger
        aria-label={accessibleLabel}
        disabled={disabled}
        className={cn(
          'grid place-items-center border border-input bg-background disabled:cursor-not-allowed disabled:opacity-40',
          size === 'sm' ? 'size-6 rounded-md' : 'size-8 rounded-lg',
        )}
      >
        {transparent ? (
          <SquareSlash
            aria-hidden='true'
            className={cn('text-muted-foreground', size === 'sm' ? 'size-3.5' : 'size-4')}
          />
        ) : (
          <span
            className={cn(
              'rounded-[3px] ring-1 ring-black/15',
              size === 'sm' ? 'size-3' : 'size-4',
            )}
            style={{ backgroundColor: draft }}
          />
        )}
      </PopoverTrigger>
      <PopoverContent side='right' align='start' className='w-60 rounded-xl p-3'>
        <div>
          <HexColorPicker
            color={draft}
            onChange={(color) => {
              const normalized = normalize(color)
              setDraft(normalized)
              setPending(normalized)
            }}
            onChangeEnd={set}
          />
        </div>
        <div className='mt-3 flex gap-2'>
          <HexColorInput
            prefixed
            color={draft}
            aria-label={t('colorPicker.hexCode')}
            className='h-8 min-w-0 flex-1 rounded-lg border border-input bg-background px-2 font-mono text-[11px] uppercase outline-none focus:border-ring'
            onChange={set}
          />
          {sampling && (
            <Button
              size='icon'
              variant='outline'
              aria-label={t('colorPicker.sampleCanvas')}
              onClick={() => {
                // Sampling replaces any uncommitted drag: a picked color commits through `set`,
                // and canceling the eyedropper leaves the committed color unchanged.
                setPending(null)
                if (value !== null) setDraft(value)
                setOpen(false)
                sampling.request(set)
              }}
            >
              <Pipette />
            </Button>
          )}
        </div>
        {allowTransparent && (
          <Button
            type='button'
            size='sm'
            variant={transparent ? 'secondary' : 'ghost'}
            className='mt-2 w-full justify-start gap-2'
            onClick={clear}
          >
            <SquareSlash aria-hidden='true' className='size-3.5 text-muted-foreground' />
            {props.emptyLabel ?? t('colorPicker.transparent')}
          </Button>
        )}
      </PopoverContent>
    </Popover>
  )
}

function normalize(value: string): string {
  const digits = value.startsWith('#') ? value.slice(1) : value
  const expanded =
    digits.length === 3 ? [...digits].map((digit) => digit.repeat(2)).join('') : digits
  return `#${expanded}`.toUpperCase()
}
