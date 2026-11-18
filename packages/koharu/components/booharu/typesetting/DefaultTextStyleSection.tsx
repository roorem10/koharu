'use client'

import { Minus, Plus } from 'lucide-react'
import { useId } from 'react'
import { useTranslation } from 'react-i18next'

import { ColorWell } from '@/components/controls/ColorWell'
import { PreferenceRow } from '@/components/preferences/PreferenceFields'
import {
  builtInLook,
  defaultTextStyleSection,
  isBuiltIn,
  isOutlineWidth,
  MAX_OUTLINE_WIDTH,
  useDefaultTextStyle,
} from '@/lib/booharu/lettering'
import { hexToRgba, rgbaToHex } from '@/lib/store'
import { Button } from '@koharu/ui/components/button'
import {
  NumberField,
  NumberFieldDecrement,
  NumberFieldGroup,
  NumberFieldIncrement,
  NumberFieldInput,
} from '@koharu/ui/components/number-field'

/** What text with no colour of its own is drawn in: Koharu's black. */
const BLACK = '#000000'
/** An outline's colour when its width is set first, as the TypeBar's border gets. */
const WHITE = '#FFFFFF'

/** The default text style, under Settings > Typesetting below Preferred fonts: the `default` of
 * `[booharu-lettering]`, the colours and outline that text added with the T tool gets, and new
 * translations too when they don't keep the original lettering's, with a Clear button per group
 * and Clear all. Unset, the text colour shows the black text is drawn in and the outline None.
 * Changes save on their own, as Koharu's settings do. */
export function DefaultTextStyleSection() {
  const { t } = useTranslation()
  const { look, loaded, change } = useDefaultTextStyle()
  const titleId = useId()
  const widthId = useId()
  const fill = look.fill ? rgbaToHex(look.fill) : BLACK
  const stroke = look.stroke ? rgbaToHex(look.stroke) : null
  const outlined = look.stroke !== null || look.stroke_width !== builtInLook.stroke_width

  return (
    // Focusable, so the colour detection switch's link can take focus to it.
    <section
      id={defaultTextStyleSection}
      tabIndex={-1}
      aria-labelledby={titleId}
      className='outline-none'
    >
      <header className='mb-3 flex items-start justify-between gap-4'>
        <div>
          <h3 id={titleId} className='text-[12px] font-semibold'>
            {t('booharu.typesetting.title')}
          </h3>
          <p className='mt-0.5 text-[11px] leading-4 text-muted-foreground'>
            {t('booharu.typesetting.description', {
              tool: t('tools.text'),
              setting: t('booharu.pipeline.detectColors'),
              tab: t('settings.tabs.pipeline'),
            })}
          </p>
          <p className='mt-0.5 text-[11px] leading-4 text-muted-foreground'>
            {t('booharu.typesetting.font', { list: t('settings.typesetting.fontFallback') })}
          </p>
        </div>
        <Button
          type='button'
          size='sm'
          variant='outline'
          className='shrink-0'
          disabled={!loaded || isBuiltIn(look)}
          onClick={() => change(builtInLook)}
        >
          {t('booharu.typesetting.clearAll')}
        </Button>
      </header>
      <div className='divide-y divide-border rounded-xl border border-border/80 bg-[var(--surface-panel)] px-5'>
        <PreferenceRow title={t('booharu.typesetting.fill')}>
          <div className='flex min-w-0 items-center gap-2'>
            <ColorWell
              label={t('booharu.typesetting.fill')}
              size='sm'
              disabled={!loaded}
              value={fill}
              // Picking the black an unset colour shows changes nothing, so it stays unset.
              onChange={(color) => color !== fill && change({ fill: hexToRgba(color) })}
            />
            <span className='min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground'>
              {fill}
            </span>
            <Button
              type='button'
              size='sm'
              variant='ghost'
              aria-label={t('booharu.typesetting.clearFill')}
              disabled={!loaded || look.fill === null}
              onClick={() => change({ fill: null })}
            >
              {t('booharu.typesetting.clear')}
            </Button>
          </div>
        </PreferenceRow>
        <PreferenceRow title={t('booharu.typesetting.outline')}>
          <div className='flex min-w-0 items-center gap-2'>
            <ColorWell
              label={t('booharu.typesetting.outlineColor')}
              size='sm'
              allowTransparent
              disabled={!loaded}
              value={stroke}
              onChange={(color) => {
                if (color === stroke) return
                // A colour picked for an outline no wide gets the built-in width, as the TypeBar's
                // border does.
                const width =
                  look.stroke_width > 0 ? {} : { stroke_width: builtInLook.stroke_width }
                change(color === null ? { stroke: null } : { stroke: hexToRgba(color), ...width })
              }}
            />
            <span className='min-w-0 flex-1 truncate text-[11px] text-muted-foreground'>
              {stroke ? <span className='font-mono'>{stroke}</span> : t('booharu.typesetting.none')}
            </span>
            <label
              htmlFor={widthId}
              aria-hidden='true'
              className='shrink-0 text-[10px] text-muted-foreground'
            >
              {t('booharu.typesetting.width')}
            </label>
            <NumberField
              id={widthId}
              name='booharu-outline-width'
              className='w-24 shrink-0'
              value={look.stroke_width}
              min={0}
              max={MAX_OUTLINE_WIDTH}
              step={0.5}
              disabled={!loaded}
              onValueChange={(width) => {
                // A cleared or half-typed field saves nothing, and shows the width again when it
                // loses focus; the field keeps a typed width within 0 to 50.
                if (!isOutlineWidth(width) || width === look.stroke_width) return
                // A width set first gives the outline a colour, white, as the TypeBar's border.
                change(
                  look.stroke || width === 0
                    ? { stroke_width: width }
                    : { stroke_width: width, stroke: hexToRgba(WHITE) },
                )
              }}
            >
              <NumberFieldGroup>
                <NumberFieldDecrement aria-label={t('booharu.typesetting.decreaseOutlineWidth')}>
                  <Minus />
                </NumberFieldDecrement>
                <NumberFieldInput aria-label={t('booharu.typesetting.outlineWidth')} />
                <NumberFieldIncrement aria-label={t('booharu.typesetting.increaseOutlineWidth')}>
                  <Plus />
                </NumberFieldIncrement>
              </NumberFieldGroup>
            </NumberField>
            <Button
              type='button'
              size='sm'
              variant='ghost'
              aria-label={t('booharu.typesetting.clearOutline')}
              disabled={!loaded || !outlined}
              onClick={() => change({ stroke: null, stroke_width: builtInLook.stroke_width })}
            >
              {t('booharu.typesetting.clear')}
            </Button>
          </div>
        </PreferenceRow>
      </div>
    </section>
  )
}
