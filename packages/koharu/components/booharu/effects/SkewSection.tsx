'use client'

import { RotateCcw } from 'lucide-react'
import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { sharedSkew, type PageEffects } from '@/lib/booharu/effects'
import { letteringOf } from '@/lib/document'
import { commands, type Layer, type SkewCounts, type Typed } from '@koharu/ipc/protocol'
import { NumberField, NumberFieldGroup, NumberFieldInput } from '@koharu/ui/components/number-field'
import { toast } from '@koharu/ui/components/toast'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** A skew's range and step along either axis, in degrees, as booharu-core's `skew::LIMIT` bounds
 * it. */
const SKEW = { min: -60, max: 60, step: 1 }

/** `degrees` brought into the skew's range, as Photoshop puts the nearest value it takes. */
const inRange = (degrees: number) => Math.min(SKEW.max, Math.max(SKEW.min, degrees))

type Axis = 'x' | 'y'

/** Skew in Text effects (S12-k): Horizontal (°) and Vertical (°), each a number box from -60 to 60
 * in steps of 1, showing the skew the selected boxes show, as the page's text effects tell it,
 * nothing, "Mixed", where they differ, and nothing while it's read. Each box sends its own axis
 * alone through `booharu_set_skew`, each box keeping its own skew along the other, and Reset skew
 * sends 0 along both, which takes the skew off; each is one undo step. A typed value goes on Enter
 * or as the box loses the focus, the nearest the range takes for one beyond it, a step of the arrow
 * keys or of a turn of the mouse wheel at once, as the type bar's other number boxes. A skew is the
 * whole box's, so the section is off while the canvas types, a tooltip saying to press Done first,
 * a new text among it, which has no layer yet; a notice counts the locked boxes and those without a
 * translation that it left as they are. */
export function SkewSection({
  targets,
  typing,
  effects,
}: {
  /** The layers the bar styles: its selected texts and blocks. */
  targets: Layer[]
  /** What the canvas types, while it does. */
  typing: Typed | null
  /** The page's text effects, by layer; undefined while they're read. */
  effects: PageEffects | undefined
}) {
  const { t } = useTranslation()
  // A block with no translation has no skew to show, as the desktop skews none.
  const lettered = targets.filter((layer) => letteringOf(layer) !== null).map(({ id }) => id)
  const shown = effects ? sharedSkew(effects, lettered) : null
  const disabled = typing !== null || targets.length === 0
  const row = useRef<HTMLDivElement>(null)
  // Counts the axis boxes' fresh starts: after a Reset, and after a skew that changed nothing, they
  // show the boxes' skew again in place of what was typed, which may be what they showed before.
  const [round, setRound] = useState(0)
  // The axis whose box had the focus at its fresh start, which its new self takes.
  const refocus = useRef<Axis | null>(null)

  useLayoutEffect(() => {
    const axis = refocus.current
    refocus.current = null
    if (axis) row.current?.querySelector<HTMLElement>(`[data-axis="${axis}"]`)?.focus()
  }, [round])

  const again = () => {
    const active = document.activeElement
    const axis = active instanceof HTMLElement ? active.dataset.axis : undefined
    refocus.current = axis === 'x' || axis === 'y' ? axis : null
    setRound((current) => current + 1)
  }

  const skew = async (x: number | null, y: number | null) => {
    const layers = targets.map(({ id }) => id)
    const counts = await call(commands.booharuSetSkew, layers, x, y).catch(() => null)
    if (counts) notice(counts)
    if (!counts?.changed) again()
  }

  const notice = ({ locked, untranslated }: SkewCounts) => {
    const notes = [
      locked > 0 ? t('booharu.effects.skewLocked', { count: locked }) : null,
      untranslated > 0 ? t('booharu.effects.skewUntranslated', { count: untranslated }) : null,
    ].filter((note): note is string => note !== null)
    if (notes.length) toast.add({ type: 'info', title: notes.join(' ') })
  }

  return (
    <Tooltip disabled={!typing}>
      <TooltipTrigger
        render={
          <div
            role='group'
            aria-label={t('booharu.effects.skew')}
            className='flex flex-col gap-2 border-t pt-2.5'
          />
        }
      >
        <span className='text-[11px] font-medium'>{t('booharu.effects.skew')}</span>
        <div ref={row} className='flex items-end gap-1.5'>
          <SkewField
            key={`x${round}`}
            axis='x'
            label={t('booharu.effects.skewX')}
            value={shown && shown.x}
            mixed={shown?.x === null}
            disabled={disabled}
            onChange={(x) => void skew(x, null)}
          />
          <SkewField
            key={`y${round}`}
            axis='y'
            label={t('booharu.effects.skewY')}
            value={shown && shown.y}
            mixed={shown?.y === null}
            disabled={disabled}
            onChange={(y) => void skew(null, y)}
          />
          <Tooltip disabled={disabled}>
            <TooltipTrigger
              render={
                <button
                  type='button'
                  aria-label={t('booharu.effects.skewReset')}
                  disabled={disabled}
                  className='grid size-6 shrink-0 place-items-center rounded-md border border-input bg-background text-muted-foreground hover:bg-muted disabled:pointer-events-none disabled:opacity-50'
                  onClick={() => {
                    again()
                    void skew(0, 0)
                  }}
                />
              }
            >
              <RotateCcw aria-hidden='true' className='size-3.5' />
            </TooltipTrigger>
            <TooltipContent>{t('booharu.effects.skewReset')}</TooltipContent>
          </Tooltip>
        </div>
      </TooltipTrigger>
      <TooltipContent>{t('booharu.effects.skewWholeBox')}</TooltipContent>
    </Tooltip>
  )
}

/** An axis's number box under its label: `value` shown, nothing where it's `null`, with "Mixed"
 * when `mixed`. A value goes once: Enter sends it, and the box's commit as it loses the focus
 * doesn't send it again, nor a value the box shows. */
function SkewField({
  axis,
  label,
  value,
  mixed,
  disabled,
  onChange,
}: {
  axis: Axis
  label: string
  value: number | null
  mixed: boolean
  disabled: boolean
  onChange: (degrees: number) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const [draft, setDraft] = useState<number | null>(value)
  // The value the box last showed or sent, as the type bar's spacing boxes keep it: the skew shows
  // only once the page's effects are read again.
  const last = useRef<number | null>(value)

  useEffect(() => {
    setDraft(value)
    last.current = value
  }, [value])

  const commit = (next: number | null) => {
    if (next === null) return setDraft(value)
    const degrees = inRange(next)
    if (degrees === last.current) return
    last.current = degrees
    onChange(degrees)
  }

  return (
    <div className='flex min-w-0 flex-1 flex-col gap-1'>
      <span className='text-[10px] font-medium text-muted-foreground'>{label}</span>
      <NumberField
        id={id}
        name={`skew-${axis}`}
        className='min-w-0'
        value={draft}
        min={SKEW.min}
        max={SKEW.max}
        step={SKEW.step}
        disabled={disabled}
        onValueChange={setDraft}
        onValueCommitted={commit}
      >
        <NumberFieldGroup>
          <NumberFieldInput
            data-axis={axis}
            aria-label={label}
            placeholder={mixed ? t('booharu.typebar.mixed') : undefined}
            onKeyDown={(event) => {
              if (event.key === 'Enter') commit(draft)
            }}
          />
        </NumberFieldGroup>
      </NumberField>
    </div>
  )
}
