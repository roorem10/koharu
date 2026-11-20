'use client'

import { useEffect, useId, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { SkewSection } from '@/components/booharu/effects/SkewSection'
import { ColorWell } from '@/components/controls/ColorWell'
import { boxActions, runBoxAction, selectedBoxes } from '@/lib/booharu/boxActions'
import { showsSkew, useTextEffects } from '@/lib/booharu/effects'
import {
  closeFx,
  commonSeed,
  openFx,
  showsGradient,
  styledTexts,
  targetSeeds,
  useFxOpen,
} from '@/lib/booharu/fx'
import {
  endColor,
  gradientShown,
  normalAngle,
  seedGradient,
  sharedColor,
  sharedPaint,
  spanStyles,
  withEndColor,
  type GradientShown,
} from '@/lib/booharu/paint'
import type { WordRange } from '@/lib/booharu/words'
import { letteringOf } from '@/lib/document'
import { usePage } from '@/lib/queries'
import { hexToRgba, rgbaToHex } from '@/lib/store'
import type {
  CharStyle,
  Gradient,
  GradientKind,
  Layer,
  TextStyle,
  Typed,
} from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { NumberField, NumberFieldGroup, NumberFieldInput } from '@koharu/ui/components/number-field'
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from '@koharu/ui/components/popover'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'
import { Switch } from '@koharu/ui/components/switch'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** A gradient's angle, in degrees, as Photoshop's runs. */
const MIN_ANGLE = -180
const MAX_ANGLE = 180

/** The effects Booharu has yet to bring back from 0.0.4's Text Effects window, shown off. */
const LATER = ['screentoneLater', 'wavyLater'] as const

/** What the type bar hands its effects: what it styles, what its characters show, and its edits. */
export interface FxProps {
  /** The layers the bar styles: its selected texts and blocks. */
  targets: Layer[]
  /** What the canvas types, while it does. */
  typing: Typed | null
  /** The words chosen in Story that the bar's character settings apply to. */
  words: WordRange | null
  /** The character values the bar shows: its draft's over what the characters share. */
  chars: CharStyle
  /** The character keys whose values the characters don't share. */
  mixed: ReadonlySet<keyof CharStyle>
  /** The bar's character edit, at its scope: the characters typed, the words chosen, or every
   * target's characters. */
  onChars: (patch: CharStyle) => void
  /** The bar's style edit giving each target its own patch from `each`, or none, in one step,
   * `patch` showing at once as what every target takes. */
  onEach: (patch: TextStyle, each: (layer: Layer) => TextStyle | null) => void
}

/** Booharu's Text effects in the type bar (S12-k): an italic "fx" button after the border's and
 * before Done, pressed while the bar's characters show a gradient or a target shows a skew, opening
 * a popover of the text's effects, as Booharu 0.0.4's Text Effects window held them: Gradient fill,
 * Skew, the effects still to come, shown off, and Reset to lettering default. It opens and closes
 * through lib/booharu/fx.ts, so FX… in a box's menu and a box's fx tag open it too. */
export function FxPopover(props: FxProps) {
  const { t } = useTranslation()
  const page = usePage().data
  const open = useFxOpen(page?.id)
  const effects = useTextEffects(page?.id).data
  const { targets, typing, words, chars, mixed } = props
  const texts = styledTexts(targets, words, typing)
  const shown = gradientShown(chars, mixed, texts?.flatMap(spanStyles) ?? null)
  const skewed = showsSkew(
    effects,
    targets.map(({ id }) => id),
  )
  const pressed = shown !== 'none' || skewed
  return (
    <Popover open={open} onOpenChange={(next) => (next ? openFx() : closeFx())}>
      <Tooltip>
        <TooltipTrigger
          render={
            <PopoverTrigger
              aria-label={t('booharu.effects.title')}
              aria-pressed={pressed}
              data-active={pressed}
              className='grid size-6 shrink-0 place-items-center rounded-md border border-input bg-background text-muted-foreground hover:bg-muted data-[active=true]:border-primary data-[active=true]:bg-primary data-[active=true]:text-primary-foreground data-[active=true]:hover:bg-primary/90'
            />
          }
        >
          <span aria-hidden='true' className='font-serif text-xs leading-none italic'>
            fx
          </span>
        </TooltipTrigger>
        <TooltipContent>{t('booharu.effects.title')}</TooltipContent>
      </Tooltip>
      <PopoverContent side='bottom' align='end' className='w-80 gap-3 rounded-xl p-3'>
        <PopoverTitle className='text-xs'>{t('booharu.effects.title')}</PopoverTitle>
        <GradientSection {...props} shown={shown} />
        <SkewSection targets={targets} typing={typing} effects={effects} />
        <div className='flex flex-col gap-1.5 border-t pt-2.5'>
          {LATER.map((effect) => (
            <div key={effect} className='flex items-center justify-between gap-2'>
              <span className='text-[11px] text-muted-foreground'>
                {t(`booharu.effects.${effect}`)}
              </span>
              <Switch size='sm' disabled aria-label={t(`booharu.effects.${effect}`)} />
            </div>
          ))}
        </div>
        <ResetFooter targets={targets} typing={typing} />
      </PopoverContent>
    </Popover>
  )
}

/** Gradient fill (S12-c, S12-k): a switch, on while every character the bar styles shows a
 * gradient and "Mixed" while only some do, then, while they all show one gradient, its kind, its
 * angle, and the colours of its ends. Turning it on gives each target a gradient seeded from its
 * own flat colour: a whole box whose style has a gradient already, which words given a colour hide,
 * gets that one back, and one showing a gradient in every character keeps its own. Off takes the
 * gradient off whole boxes, so their flat colour shows again, and gives typed characters or chosen
 * words the flat colour they show, which beats the gradient they'd have. An edit of the gradient
 * writes it, changed, to each target showing it, never to one without a gradient. */
export function GradientSection({
  targets,
  typing,
  words,
  chars,
  mixed,
  shown,
  onChars,
  onEach,
}: FxProps & {
  /** Whether the characters show a gradient. */
  shown: GradientShown
}) {
  const { t } = useTranslation()
  const mixedId = useId()
  const wholeBoxes = !typing && !words
  const paint = sharedPaint(chars, mixed)
  const gradient = paint && 'gradient' in paint ? paint.gradient : null
  // The text's own colour, which typed characters or chosen words whose colours differ take.
  const base = targets.map(letteringOf).find((text) => text !== null)?.style?.chars
  const kinds: Record<GradientKind, string> = {
    linear: t('booharu.effects.linear'),
    radial: t('booharu.effects.radial'),
  }

  const turnOn = () => {
    if (!wholeBoxes)
      return onChars({ fill_gradient: seedGradient(sharedColor(chars, mixed, base)) })
    const seeds = targetSeeds(targets)
    if (seeds.size === 0) return
    const common = commonSeed(seeds, targets.length)
    onEach({ chars: common ? { fill_gradient: common } : {} }, (layer) => {
      const seed = seeds.get(layer.id)
      return seed ? { chars: { fill_gradient: seed } } : null
    })
  }

  const turnOff = () =>
    onChars(wholeBoxes ? { fill_gradient: null } : { fill: sharedColor(chars, mixed, base) })

  const change = (next: Gradient) => {
    if (!wholeBoxes) return onChars({ fill_gradient: next })
    onEach({ chars: { fill_gradient: next } }, (layer) =>
      showsGradient(letteringOf(layer)) ? { chars: { fill_gradient: next } } : null,
    )
  }

  return (
    <div className='flex flex-col gap-2'>
      <div className='flex items-center justify-between gap-2'>
        <span className='text-[11px] font-medium'>{t('booharu.effects.gradient')}</span>
        <div className='flex items-center gap-1.5'>
          {shown === 'some' && (
            <span id={mixedId} className='text-[10px] text-muted-foreground'>
              {t('booharu.typebar.mixed')}
            </span>
          )}
          <Switch
            size='sm'
            aria-label={t('booharu.effects.gradient')}
            aria-describedby={shown === 'some' ? mixedId : undefined}
            checked={shown === 'all'}
            onCheckedChange={(on) => (on ? turnOn() : turnOff())}
          />
        </div>
      </div>
      {gradient && (
        <div className='grid grid-cols-2 gap-x-2 gap-y-1.5'>
          <Field label={t('booharu.effects.kind')}>
            <Select
              value={gradient.kind}
              items={kinds}
              onValueChange={(kind) =>
                kind &&
                kind !== gradient.kind &&
                change({ ...gradient, kind: kind as GradientKind })
              }
            >
              <SelectTrigger size='sm' aria-label={t('booharu.effects.kind')} className='w-full'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent
                // A pick with the pointer leaves the keys to the canvas, as the border's method's.
                finalFocus={(closed) => closed === 'keyboard'}
              >
                {(Object.keys(kinds) as GradientKind[]).map((kind) => (
                  <SelectItem key={kind} value={kind}>
                    {kinds[kind]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field label={t('booharu.effects.angle')}>
            <AngleField
              value={normalAngle(gradient.angle ?? 0)}
              // A radial gradient has no direction.
              disabled={gradient.kind !== 'linear'}
              onChange={(angle) => change({ ...gradient, angle })}
            />
          </Field>
          {(['start', 'end'] as const).map((end) => (
            <Field key={end} label={t(`booharu.effects.${end}`)}>
              <ColorWell
                label={t(`booharu.effects.${end}`)}
                size='sm'
                value={rgbaToHex(endColor(gradient, end))}
                onChange={(color) => {
                  if (color && color !== rgbaToHex(endColor(gradient, end))) {
                    change(withEndColor(gradient, end, hexToRgba(color)))
                  }
                }}
              />
            </Field>
          ))}
        </div>
      )}
    </div>
  )
}

/** A control under its small label. */
function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className='flex min-w-0 flex-col gap-1'>
      <span className='text-[10px] font-medium text-muted-foreground'>{label}</span>
      {children}
    </div>
  )
}

/** The gradient's angle, from -180 to 180 degrees in steps of one: a typed angle goes on Enter or
 * as the box loses the focus, a step of the arrow keys or the mouse wheel at once, as the type
 * bar's other number boxes. */
function AngleField({
  value,
  disabled,
  onChange,
}: {
  value: number
  disabled: boolean
  onChange: (angle: number) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const [draft, setDraft] = useState<number | null>(value)

  useEffect(() => setDraft(value), [value])

  const commit = (next: number | null) => {
    if (next === null || next < MIN_ANGLE || next > MAX_ANGLE) return setDraft(value)
    if (next !== value) onChange(next)
  }

  return (
    <NumberField
      id={id}
      name='gradient-angle'
      className='min-w-0'
      value={draft}
      min={MIN_ANGLE}
      max={MAX_ANGLE}
      step={1}
      disabled={disabled}
      onValueChange={setDraft}
      onValueCommitted={commit}
    >
      <NumberFieldGroup>
        <NumberFieldInput
          aria-label={t('booharu.effects.angle')}
          onKeyDown={(event) => {
            if (event.key === 'Enter') commit(draft)
          }}
        />
      </NumberFieldGroup>
    </NumberField>
  )
}

/** The box action Reset to lettering default, which the popover's footer runs. */
const reset = boxActions.find((action) => action.id === 'reset')

/** Reset to lettering default over the bar's targets, as a box's menu runs it; off while the
 * canvas types, and where it has nothing to reset. */
function ResetFooter({ targets, typing }: { targets: Layer[]; typing: Typed | null }) {
  const { t } = useTranslation()
  const page = usePage().data
  const selection = page
    ? {
        page: page.id,
        boxes: selectedBoxes(
          page.layers,
          targets.map(({ id }) => id),
        ),
      }
    : null
  const enabled = !typing && !!reset && !!selection && reset.enabled(selection)
  return (
    <div className='flex border-t pt-2.5'>
      <Button
        type='button'
        size='sm'
        variant='outline'
        disabled={!enabled}
        onClick={() => {
          if (reset && selection) runBoxAction(reset, selection)
        }}
      >
        {t('booharu.boxes.reset')}
      </Button>
    </div>
  )
}
