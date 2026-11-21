'use client'

import { Pipette, SquareSlash } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { HexColorInput, HexColorPicker } from 'react-colorful'
import { useTranslation } from 'react-i18next'

import { useColorSampling } from '@/components/controls/ColorSampling'
import { toHex, toRgba } from '@/lib/color'
import { rememberColor, useColorHistory } from '@/lib/colorHistory'
import { Button } from '@koharu/ui/components/button'
import { Popover, PopoverContent, PopoverTrigger } from '@koharu/ui/components/popover'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'
import { cn } from '@koharu/ui/lib/utils'

type ColorWellProps = {
  label?: string
  size?: 'default' | 'sm'
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
  // Booharu: a picker drag can move the colour without its end event firing, for example when it
  // ends in the black corner (koharu#866). The dragged colour waits here, apart from `value`, so
  // closing the picker applies it and Escape discards it. Every other way of settling the colour
  // (a finished drag, a typed hex code, a recent colour, Transparent, the eyedropper or a new value
  // from outside) clears it, so it never overrides a later choice.
  const [pending, setPending] = useState<string | null>(null)
  const [open, setOpen] = useState(false)
  const [typed, setTyped] = useState<{ text: string; color: string } | null>(null)
  const opened = useRef(draft)
  const sampling = useColorSampling()
  const recent = useColorHistory((state) => state.colors)
  const accessibleLabel = label ?? t('colorPicker.brushColor')

  useEffect(() => {
    setPending(null) // Booharu: a new value from outside settles the colour.
    if (value === null) {
      setTransparent(true)
    } else {
      setDraft(value)
      setTransparent(false)
    }
  }, [value])

  const set = (color: string) => {
    const normalized = toHex(toRgba(color))
    setDraft(normalized)
    setPending(null) // Booharu: a committed colour settles any unfinished drag.
    setTransparent(false)
    if (props.allowTransparent) props.onChange(normalized)
    else props.onChange(normalized)
    return normalized
  }

  const clear = () => {
    if (!props.allowTransparent) return
    setPending(null) // Booharu: Transparent settles any unfinished drag.
    setTransparent(true)
    props.onChange(null)
  }

  // A colour joins the history when the popover closes on it, so the drags and the keystrokes of a
  // hex code that led there don't fill it; canvas picks join it in `ColorSamplingProvider`.
  const openChange = (next: boolean) => {
    if (next) opened.current = draft
    else if (!transparent && draft !== opened.current) rememberColor(draft)
    setOpen(next)
  }

  return (
    <Popover
      open={open}
      // Booharu: closing the picker applies a drag whose end event never came and remembers it, as
      // Koharu remembers the colour a well closes on; Escape discards it, shows the committed colour
      // again and remembers nothing. With no drag waiting, Koharu's own handler runs.
      onOpenChange={(next, details) => {
        if (next || pending === null) return openChange(next)
        setPending(null)
        if (details.reason === 'escape-key') {
          if (value !== null) setDraft(value)
        } else {
          const applied = set(pending)
          if (applied !== opened.current) rememberColor(applied)
        }
        setOpen(false)
      }}
    >
      <Tooltip>
        <TooltipTrigger
          render={
            <PopoverTrigger
              aria-label={accessibleLabel}
              disabled={disabled}
              // A small well is a field among a bar's fields; a large one is a bare swatch, as
              // Photoshop's toolbar shows its foreground colour.
              className={cn(
                'grid place-items-center outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-40',
                size === 'sm'
                  ? 'size-6 rounded-md border border-input bg-background'
                  : 'size-8 rounded-md',
              )}
            />
          }
        >
          {transparent ? (
            <SquareSlash
              aria-hidden='true'
              className={cn('text-muted-foreground', size === 'sm' ? 'size-3.5' : 'size-4')}
            />
          ) : (
            <span
              className={cn(
                size === 'sm'
                  ? 'size-3 rounded-[3px] ring-1 ring-black/15'
                  : 'size-5 rounded-[6px] border border-foreground/40 shadow-[inset_0_0_0_1px_white]',
              )}
              style={{ backgroundColor: draft }}
            />
          )}
        </TooltipTrigger>
        {/* Beside the tool rail's tools, as theirs are. */}
        <TooltipContent side={size === 'sm' ? undefined : 'right'}>
          {accessibleLabel}
        </TooltipContent>
      </Tooltip>
      <PopoverContent side='right' align='start' className='w-60 p-3'>
        <div>
          <HexColorPicker
            color={draft}
            onChange={(color) => {
              const normalized = toHex(toRgba(color))
              setDraft(normalized)
              setPending(normalized) // Booharu: kept until the drag ends or the picker closes.
            }}
            onChangeEnd={set}
          />
        </div>
        <div className='mt-3 flex gap-2'>
          {/* The field resets its text whenever `color` changes, so it keeps the text typed while
              that names the colour applied: the expanded colour would turn a typed `ff0` into
              `FFFF00` and cut off the code's remaining digits. */}
          <HexColorInput
            prefixed
            color={typed?.color === draft ? typed.text : draft}
            aria-label={t('colorPicker.hexCode')}
            className='h-8 min-w-0 flex-1 rounded-lg border border-input bg-background px-2 font-mono text-[11px] uppercase outline-none focus:border-ring'
            onChange={(text) => setTyped({ text, color: set(text) })}
          />
          {sampling && (
            <Button
              size='icon'
              variant='outline'
              aria-label={t('colorPicker.sampleCanvas')}
              onClick={() => {
                // Booharu: sampling replaces an unfinished drag; a sample commits through `set`,
                // and cancelling the eyedropper leaves the committed colour.
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
        {recent.length > 0 && (
          <div role='group' aria-label={t('colorPicker.recent')} className='mt-3 flex gap-1'>
            {recent.map((color) => (
              <button
                key={color}
                type='button'
                aria-label={t('colorPicker.recentColor', { color })}
                className='size-5 rounded-[3px] ring-1 ring-black/15 outline-none focus-visible:ring-2 focus-visible:ring-ring'
                style={{ backgroundColor: color }}
                onClick={() => set(color)}
              />
            ))}
          </div>
        )}
        {allowTransparent && (
          <Button
            type='button'
            size='sm'
            variant={transparent ? 'secondary' : 'ghost'}
            className='mt-2 w-full justify-start gap-2'
            onClick={clear}
          >
            <SquareSlash aria-hidden='true' className='size-3.5 text-muted-foreground' />
            {t('colorPicker.transparent')}
          </Button>
        )}
      </PopoverContent>
    </Popover>
  )
}
