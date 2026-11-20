'use client'

import { Fragment, useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { ColorWell } from '@/components/controls/ColorWell'
import { FontPicker } from '@/components/controls/FontPicker'
import { PreferenceRow } from '@/components/preferences/PreferenceFields'
import { categoryName, letteringCategories } from '@/lib/booharu/categories'
import {
  EMPHASIS_PERCENT,
  EMPHASIS_WEIGHT,
  EMPHASIS_WIDTH,
  emphasisFactor,
  emphasisPercent,
  emphasisWeight,
  emphasisWidth,
  NO_EMPHASIS,
  ownLook,
  resetCategory,
  setAi,
  setEmphasis,
  useEmphasisSettings,
  type EmphasisChoice,
  type EmphasisKey,
  type EmphasisLook,
} from '@/lib/booharu/emphasis'
import { useBooharuFontPicker } from '@/lib/booharu/fonts'
import { useFonts } from '@/lib/queries'
import { hexToRgba, rgbaToHex } from '@/lib/store'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import { NumberField, NumberFieldGroup, NumberFieldInput } from '@koharu/ui/components/number-field'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

/** What Settings shows, in its list's order: the default, then every lettering category. */
const choices: EmphasisChoice[] = ['default', ...letteringCategories]

/** Whether stressed words are italic, as the Italic list offers it: as the text or the default
 * has them, upright, or italic. */
type Slant = 'inherit' | 'normal' | 'italic'

/** The colours an emphasis sets: its text's and its two outlines'. */
type Colour = 'fill' | 'stroke' | 'outer_stroke'

/** Each colour's strings, and the colour its well shows while neither it nor the default sets
 * one: Koharu's black text and white outline. */
const colourRows = {
  fill: { name: 'color', standIn: '#000000' },
  stroke: { name: 'stroke_color', standIn: '#FFFFFF' },
  outer_stroke: { name: 'outer_stroke_color', standIn: '#FFFFFF' },
} as const satisfies Record<Colour, { name: string; standIn: string }>

/** Settings > Typesetting's Emphasis card, after the lettering categories (D25):
 * `[booharu-emphasis]`, how the words the type bar's star or the AI stresses look. A box turns the
 * AI's emphasis on or off, which only general chat models use; a list chooses the default emphasis
 * or a lettering category, whose own values show under it: the font, weight, italic, size as a
 * percentage of the text's, colour, and the two outlines' colours and widths. A value the default
 * leaves unset uses the box's style, and one a category leaves unset the default's; a cleared
 * field, the font list's first row or a colour's Clear sets it so, and a category's own values can
 * all be taken away at once. Each change saves at once, as 0.0.4's card did. */
export function EmphasisPreferences() {
  const { t } = useTranslation()
  const { settings, change } = useEmphasisSettings()
  const fonts = useFonts()
  const booharuFonts = useBooharuFontPicker()
  const [choice, setChoice] = useState<EmphasisChoice>('default')
  const titleId = useId()
  const aiId = useId()
  const hintId = useId()
  const loaded = settings !== undefined
  const look = settings ? ownLook(settings, choice) : NO_EMPHASIS
  const base = settings ? ownLook(settings, 'default') : NO_EMPHASIS
  const own = choice !== 'default' && settings?.categories?.[choice] !== undefined
  const inherit = t(
    choice === 'default' ? 'booharu.emphasis.useBoxStyle' : 'booharu.emphasis.sameAsDefault',
  )
  const names = Object.fromEntries(
    choices.map((candidate) => [
      candidate,
      candidate === 'default' ? t('booharu.emphasis.default') : t(categoryName(candidate)),
    ]),
  ) as Record<EmphasisChoice, string>
  const slants: Record<Slant, string> = {
    inherit,
    normal: t('booharu.emphasis.normal'),
    italic: t('booharu.emphasis.italic'),
  }
  const slant: Slant = look.italic === null ? 'inherit' : look.italic ? 'italic' : 'normal'

  const set = <Key extends EmphasisKey>(key: Key, value: EmphasisLook[Key]) =>
    void change(setEmphasis(choice, key, value))

  /** Saves `key` as a committed number field has it: none for a cleared field, else `value` as
   * `taken` takes it, unless it takes none or it's the value set already. */
  const commit = (
    key: 'weight' | 'size_factor' | 'stroke_width' | 'outer_stroke_width',
    value: number | null,
    taken: (value: number) => number | null,
  ) => {
    if (value === null) {
      if (look[key] !== null) set(key, null)
      return
    }
    const next = taken(value)
    if (next !== null && next !== look[key]) set(key, next)
  }

  const colourRow = (key: Colour) => {
    const { name, standIn } = colourRows[key]
    const value = look[key]
    const hex = value ? rgbaToHex(value) : null
    // A category without its own shows the default's.
    const shown = hex ?? (base[key] ? rgbaToHex(base[key]) : standIn)
    return (
      <PreferenceRow title={t(`booharu.emphasis.${name}`)}>
        <div className='flex min-w-0 items-center gap-2'>
          <ColorWell
            label={t(`booharu.emphasis.labels.${name}`)}
            size='sm'
            disabled={!loaded}
            value={shown}
            // A colour picked while none is set is saved, even the one the well shows.
            onChange={(color) => (hex === null || color !== hex) && set(key, hexToRgba(color))}
          />
          <span className='min-w-0 flex-1 truncate text-[11px] text-muted-foreground'>
            {hex ? <span className='font-mono'>{hex}</span> : inherit}
          </span>
          <Button
            type='button'
            size='sm'
            variant='ghost'
            aria-label={t(`booharu.emphasis.clearLabels.${name}`)}
            disabled={!loaded || hex === null}
            onClick={() => set(key, null)}
          >
            {t('booharu.emphasis.clear')}
          </Button>
        </div>
      </PreferenceRow>
    )
  }

  return (
    <section aria-labelledby={titleId}>
      <header className='mb-3'>
        <h3 id={titleId} className='text-[12px] font-semibold'>
          {t('booharu.emphasis.title')}
        </h3>
        <p className='mt-0.5 text-[11px] leading-4 text-muted-foreground'>
          {t('booharu.emphasis.description')}
        </p>
      </header>
      <div className='divide-y divide-border rounded-xl border border-border/80 bg-[var(--surface-panel)] px-5'>
        {/* A preference row, its hint read with the box. */}
        <div className='grid grid-cols-[minmax(150px,0.9fr)_minmax(220px,1.1fr)] items-center gap-6 py-3.5'>
          <div>
            <p id={aiId} className='text-[12px] font-medium'>
              {t('booharu.emphasis.enabledForAi')}
            </p>
            <p id={hintId} className='mt-0.5 text-[10px] leading-4 text-muted-foreground'>
              {t('booharu.emphasis.aiHint')}
            </p>
          </div>
          <div className='min-w-0'>
            <Checkbox
              aria-labelledby={aiId}
              aria-describedby={hintId}
              checked={settings?.ai ?? false}
              disabled={!loaded}
              onCheckedChange={(on) => void change(setAi(on))}
            />
          </div>
        </div>
        <PreferenceRow title={t('booharu.emphasis.category')}>
          <Select
            value={choice}
            items={names}
            onValueChange={(next) => next && setChoice(next as EmphasisChoice)}
          >
            <SelectTrigger
              aria-label={t('booharu.emphasis.labels.category')}
              className='h-8 w-full text-[11px]'
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {choices.map((candidate) => (
                <SelectItem key={candidate} value={candidate} label={names[candidate]}>
                  {names[candidate]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </PreferenceRow>
        {/* Each choice's own fields, so one's half-typed number never shows under another. */}
        <Fragment key={choice}>
          <PreferenceRow title={t('booharu.emphasis.font')}>
            <FontPicker
              value={look.family ?? ''}
              families={fonts.data ?? []}
              disabled={!loaded}
              ariaLabel={t('booharu.emphasis.labels.font')}
              // The leading row stands for no family of its own.
              onChange={(family) => set('family', family || null)}
              {...booharuFonts}
              noneLabel={inherit}
            />
          </PreferenceRow>
          <PreferenceRow title={t('booharu.emphasis.weight')}>
            <OptionalNumber
              label={t('booharu.emphasis.labels.weight')}
              value={look.weight}
              placeholder={inherit}
              {...EMPHASIS_WEIGHT}
              format={{ maximumFractionDigits: 0, useGrouping: false }}
              disabled={!loaded}
              onCommit={(weight) => commit('weight', weight, emphasisWeight)}
            />
          </PreferenceRow>
          <PreferenceRow title={t('booharu.emphasis.italic')}>
            <Select
              value={slant}
              items={slants}
              disabled={!loaded}
              onValueChange={(next) => {
                if (!next || next === slant) return
                set('italic', next === 'inherit' ? null : next === 'italic')
              }}
            >
              <SelectTrigger
                aria-label={t('booharu.emphasis.labels.italic')}
                className='h-8 w-full text-[11px]'
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {(Object.keys(slants) as Slant[]).map((candidate) => (
                  <SelectItem key={candidate} value={candidate} label={slants[candidate]}>
                    {slants[candidate]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </PreferenceRow>
          <PreferenceRow title={t('booharu.emphasis.sizePercent')}>
            <OptionalNumber
              label={t('booharu.emphasis.labels.sizePercent')}
              value={look.size_factor === null ? null : emphasisPercent(look.size_factor)}
              placeholder={inherit}
              {...EMPHASIS_PERCENT}
              format={{ maximumFractionDigits: 2 }}
              disabled={!loaded}
              onCommit={(percent) => commit('size_factor', percent, emphasisFactor)}
            />
          </PreferenceRow>
          {colourRow('fill')}
          {colourRow('stroke')}
          <PreferenceRow title={t('booharu.emphasis.stroke_width')}>
            <OptionalNumber
              label={t('booharu.emphasis.labels.stroke_width')}
              value={look.stroke_width}
              placeholder={inherit}
              {...EMPHASIS_WIDTH}
              format={{ maximumFractionDigits: 2 }}
              disabled={!loaded}
              onCommit={(width) => commit('stroke_width', width, emphasisWidth)}
            />
          </PreferenceRow>
          {colourRow('outer_stroke')}
          <PreferenceRow title={t('booharu.emphasis.outer_stroke_width')}>
            <OptionalNumber
              label={t('booharu.emphasis.labels.outer_stroke_width')}
              value={look.outer_stroke_width}
              placeholder={inherit}
              {...EMPHASIS_WIDTH}
              format={{ maximumFractionDigits: 2 }}
              disabled={!loaded}
              onCommit={(width) => commit('outer_stroke_width', width, emphasisWidth)}
            />
          </PreferenceRow>
          {choice !== 'default' && (
            <div className='flex justify-end py-3.5'>
              <Button
                type='button'
                size='sm'
                variant='outline'
                disabled={!loaded || !own}
                onClick={() => void change(resetCategory(choice))}
              >
                {t('booharu.emphasis.resetCategory')}
              </Button>
            </div>
          )}
        </Fragment>
      </div>
    </section>
  )
}

/** A number field for one of an emphasis's values, `value`, empty while it sets none and showing
 * `placeholder`, what it uses then. A typed number is committed as the field loses focus, on
 * Enter or as an arrow's step ends, the field keeping it within `min` to `max`; a cleared field
 * commits none. */
function OptionalNumber({
  label,
  value,
  placeholder,
  min,
  max,
  step,
  format,
  disabled,
  onCommit,
}: {
  label: string
  value: number | null
  placeholder: string
  min: number
  max: number
  step: number
  format: Intl.NumberFormatOptions
  disabled: boolean
  onCommit: (value: number | null) => void
}) {
  // The number while the field is edited; undefined while it isn't.
  const [editing, setEditing] = useState<number | null | undefined>(undefined)
  return (
    <NumberField
      className='w-full'
      value={editing === undefined ? value : editing}
      min={min}
      max={max}
      step={step}
      format={format}
      disabled={disabled}
      onValueChange={setEditing}
      onValueCommitted={(next) => {
        setEditing(undefined)
        if (next !== value) onCommit(next)
      }}
    >
      <NumberFieldGroup>
        <NumberFieldInput aria-label={label} placeholder={placeholder} />
      </NumberFieldGroup>
    </NumberField>
  )
}
