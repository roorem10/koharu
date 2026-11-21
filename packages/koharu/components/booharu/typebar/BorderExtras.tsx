'use client'

import { ArrowRightLeft, Minus, MoreHorizontal, Plus } from 'lucide-react'
import { useEffect, useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { WholeBoxOnly } from '@/components/booharu/typebar/AppliesTo'
import { Choice } from '@/components/controls/Choice'
import { ColorWell } from '@/components/controls/ColorWell'
import type { BorderKey } from '@/lib/booharu/mixed'
import { isLight, sharedPaint } from '@/lib/booharu/paint'
import type { WordRange } from '@/lib/booharu/words'
import { toHex, toRgba } from '@/lib/color'
import { useKoharuStore } from '@/lib/store'
import type { CharKey, CharStyle, StrokeMethod, TextStyle } from '@koharu/ipc/protocol'
import {
  NumberField,
  NumberFieldDecrement,
  NumberFieldGroup,
  NumberFieldIncrement,
  NumberFieldInput,
} from '@koharu/ui/components/number-field'
import { Popover, PopoverContent, PopoverTrigger } from '@koharu/ui/components/popover'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

const BLACK = '#000000'
const WHITE = '#FFFFFF'

/** What the type bar hands its border extras: the values its characters share, and its edits. */
export interface BorderExtrasProps {
  /** What the characters the bar styles share. */
  chars: CharStyle
  /** The keys whose values they don't share. */
  mixed: ReadonlySet<CharKey>
  /** Whether every character has a first border of its own, which a second border goes around. */
  bordered: boolean
  /** The targets' outline method, `null` while they differ. */
  method: StrokeMethod | null
  /** Whether there's a paragraph for the method: a new text being typed has none yet. */
  paragraph: boolean
  /** The words chosen in the Story panel that the bar's character settings apply to. */
  words: WordRange | null
  /** The bar's character setting: the characters typed, the words chosen, or every target's. */
  onChars: (patch: CharStyle) => void
  /** The bar's border setting, `patch` with `companion`, the colour or width that goes with it,
   * which each box keeps its own of, `own` where the characters have none. */
  onBorder: (patch: CharStyle, companion: BorderKey, own: CharStyle) => void
  /** The bar's paragraph setting, which every target takes. */
  onStyle: (patch: TextStyle) => void
}

/** Booharu's border extras in the type bar, beside Koharu's border colour and width: a small
 * button whose popover holds what Booharu 0.89's Border popover added to Koharu's border. The 2nd
 * border goes around the first, so it's off on characters without one; its colour and width show
 * nothing, "Mixed", where the characters' differ, and each box keeps its own of the one not set,
 * as the first border's do (S11-h). Its width goes as Koharu 0.90's border width does: a typed
 * width on Enter or as the box loses the focus, a step at once, and an empty box shows the width
 * again. Swap trades the text's colour and its border's, off while a gradient shows or the
 * colours or widths differ. The outline method, Koharu's border by expansion first as the
 * default, then Contour, is the whole box's (S11-f), as the paragraph's settings are, and shows
 * nothing while the targets' methods differ. */
export function BorderExtras({
  chars,
  mixed,
  bordered,
  method,
  paragraph,
  words,
  onChars,
  onBorder,
  onStyle,
}: BorderExtrasProps) {
  const { t } = useTranslation()
  const typing = useKoharuStore((state) => state.typing !== null)
  const widthId = useId()
  // The colour, or the gradient, every character shows, none while they differ (S12-c).
  const paint = sharedPaint(chars, mixed)
  const outerWidth = chars.outer_stroke_width ?? 0
  const outer = chars.outer_stroke ?? null
  const outerEnabled = bordered && outer !== null && outer[3] > 0 && outerWidth > 0
  // Empty while the widths differ (S11-h), 3 where there's none.
  const shownWidth = mixed.has('outer_stroke_width') ? null : outerWidth > 0 ? outerWidth : 3
  // What the width box shows while a width is typed, as Koharu 0.90's border width holds it.
  const [width, setWidth] = useState<number | null>(shownWidth)

  useEffect(() => {
    setWidth(shownWidth)
  }, [shownWidth])

  const commitWidth = (next: number | null) => {
    if (next === null || next < 0.5 || next > 32) return setWidth(shownWidth)
    // Colours that differ stay each character's own, and each box's own, black only where one has
    // none (S11-h).
    onBorder(
      { outer_stroke_width: next },
      'outer_stroke',
      outer ? {} : { outer_stroke: toRgba(BLACK) },
    )
  }
  // Koharu's border, the default, listed first.
  const methods: Record<StrokeMethod, string> = {
    expansion: t('booharu.borders.outlineExpansion'),
    contour: t('booharu.borders.outlineContour'),
  }
  const heading = 'text-[10px] font-medium text-muted-foreground'
  return (
    <Popover>
      <Tooltip>
        <TooltipTrigger
          render={
            <PopoverTrigger
              aria-label={t('booharu.borders.more')}
              className='grid size-6 shrink-0 place-items-center rounded-md border border-input bg-background text-muted-foreground hover:bg-muted'
            />
          }
        >
          <MoreHorizontal aria-hidden='true' className='size-3.5' />
        </TooltipTrigger>
        <TooltipContent>{t('booharu.borders.more')}</TooltipContent>
      </Tooltip>
      <PopoverContent side='bottom' align='end' className='w-56 gap-1.5 p-2'>
        <span className={heading}>{t('booharu.borders.secondBorder')}</span>
        <div className='flex items-center gap-1'>
          <ColorWell
            label={t('booharu.borders.secondBorderColor')}
            size='sm'
            allowTransparent
            disabled={!bordered}
            value={
              // The colour shared while only the widths differ (S11-h).
              mixed.has('outer_stroke')
                ? ''
                : bordered &&
                    outer &&
                    outer[3] > 0 &&
                    (outerWidth > 0 || mixed.has('outer_stroke_width'))
                  ? toHex(outer)
                  : null
            }
            onChange={(color) => {
              // Both keys go, so the text is as Koharu writes it again. While typing, or for words
              // chosen in Story, a mark can't unset them, so characters with no second border are
              // left unmarked.
              if (color === null) {
                const some =
                  outerEnabled || mixed.has('outer_stroke') || mixed.has('outer_stroke_width')
                if (some || (!typing && !words)) {
                  onChars({ outer_stroke: null, outer_stroke_width: null })
                }
                return
              }
              // Widths that differ stay each character's own, and each box's own, 3 only where
              // one has none (S11-h).
              onBorder(
                { outer_stroke: toRgba(color) },
                'outer_stroke_width',
                outerWidth > 0 ? {} : { outer_stroke_width: 3 },
              )
            }}
          />
          <NumberField
            id={widthId}
            name='second-border-width'
            className='min-w-0 flex-1'
            value={width}
            min={0.5}
            max={32}
            step={0.5}
            disabled={!bordered}
            onValueChange={setWidth}
            onValueCommitted={commitWidth}
          >
            <NumberFieldGroup>
              <NumberFieldDecrement aria-label={t('booharu.borders.decreaseSecondBorderWidth')}>
                <Minus />
              </NumberFieldDecrement>
              <NumberFieldInput
                aria-label={t('booharu.borders.secondBorderWidth')}
                placeholder={
                  mixed.has('outer_stroke_width') ? t('booharu.typebar.mixed') : undefined
                }
              />
              <NumberFieldIncrement aria-label={t('booharu.borders.increaseSecondBorderWidth')}>
                <Plus />
              </NumberFieldIncrement>
            </NumberFieldGroup>
          </NumberField>
        </div>
        <button
          type='button'
          // Not while a gradient shows, or the paints differ, the fill being the paint (S12-c).
          disabled={
            !(paint && 'color' in paint) || mixed.has('stroke') || mixed.has('stroke_width')
          }
          className='flex h-6 items-center gap-1.5 rounded-md border border-input bg-background px-2 text-xs text-muted-foreground hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-50'
          onClick={() => onChars(swappedColors(chars))}
        >
          <ArrowRightLeft aria-hidden='true' className='size-3.5 shrink-0' />
          <span className='truncate'>{t('booharu.borders.swapFillOutline')}</span>
        </button>
        <span className={heading}>
          {t('booharu.borders.outlineMethod')}
          {/* The whole box's, while typing or with words chosen in Story (S11-f). */}
          <WholeBoxOnly words={words} />
        </span>
        <Choice
          size='sm'
          value={method ?? ''}
          choices={methods}
          label={t('booharu.borders.outlineMethod')}
          placeholder={t('booharu.typebar.mixed')} // "Mixed" while the methods differ (S11-h).
          disabled={!paragraph}
          className='w-full'
          // Expansion, Koharu's border, is what no method means, so the text stays readable by
          // Koharu.
          onChange={(next) => onStyle({ stroke_method: next === 'expansion' ? null : next })}
        />
      </PopoverContent>
    </Popover>
  )
}

/** `chars` with the text's colour and its border's swapped, as Booharu 0.0.4 swapped them. Text
 * without a visible border gets one in its colour, at the bar's default width, over a black or
 * white fill, whichever stands out from that colour. The new fill takes off any gradient, as a
 * colour picked in the colour well does, a fill and a gradient fill being one paint (S12-c), so a
 * box gradient its words' own colours hid doesn't show again. */
export function swappedColors(chars: CharStyle): CharStyle {
  const fill = chars.fill ?? toRgba(BLACK)
  const stroke = chars.stroke ?? null
  const width = chars.stroke_width ?? 0
  if (stroke && stroke[3] > 0 && width > 0)
    return { fill: stroke, fill_gradient: null, stroke: fill }
  const [red, green, blue] = fill
  return {
    fill: toRgba(isLight(fill) ? BLACK : WHITE),
    fill_gradient: null,
    stroke: [red, green, blue, 1],
    stroke_width: width > 0 ? width : 1.5,
  }
}
