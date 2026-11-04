'use client'

import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { ColorWell } from '@/components/controls/ColorWell'
import { FontPicker } from '@/components/controls/FontPicker'
import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { call } from '@/lib/backend'
import { hexToRgba, rgbaToHex } from '@/lib/color'
import { LETTERING_CATEGORIES } from '@/lib/line-labels'
import { useFonts } from '@/lib/queries'
import {
  commands,
  type EmphasisPreferences as EmphasisSettings,
  type RunStyle,
} from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import { Input } from '@koharu/ui/components/input'
import { NativeSelect, NativeSelectOption } from '@koharu/ui/components/native-select'

import { emptyEmphasisStyle } from './settings'

export function EmphasisPreferences() {
  const { t } = useTranslation()
  const fonts = useFonts()
  const [settings, setSettings] = useState<EmphasisSettings | null>(null)
  const [category, setCategory] = useState('default')
  const latest = useRef<EmphasisSettings | null>(null)
  const queue = useRef<Promise<unknown>>(Promise.resolve())
  const version = useRef(0)
  const mounted = useRef(false)
  useEffect(() => {
    mounted.current = true
    let active = true
    void call(commands.getEmphasisSettings)
      .then((saved) => {
        if (active) {
          latest.current = saved
          setSettings(saved)
        }
      })
      .catch(() => undefined)
    return () => {
      active = false
      mounted.current = false
    }
  }, [])

  const save = (update: (previous: EmphasisSettings) => EmphasisSettings) => {
    if (!latest.current) return
    const next = update(latest.current)
    latest.current = next
    setSettings(next)
    const generation = ++version.current
    queue.current = queue.current
      .catch(() => undefined)
      .then(() => call(commands.saveEmphasisSettings, next))
      .then((saved) => {
        if (mounted.current && version.current === generation) {
          latest.current = saved
          setSettings(saved)
        }
      })
      .catch(async () => {
        if (mounted.current && version.current === generation) {
          const saved = await call(commands.getEmphasisSettings).catch(() => null)
          if (saved && mounted.current && version.current === generation) {
            latest.current = saved
            setSettings(saved)
          }
        }
      })
  }

  if (!settings) return null
  const style =
    category === 'default'
      ? settings.default
      : (settings.categories[category] ?? emptyEmphasisStyle)
  const inherit = t(category === 'default' ? 'emphasis.useBoxStyle' : 'emphasis.sameAsDefault')
  const change = (patch: Partial<RunStyle>) => {
    save((previous) => {
      if (category === 'default') return { ...previous, default: { ...previous.default, ...patch } }
      return {
        ...previous,
        categories: {
          ...previous.categories,
          [category]: { ...emptyEmphasisStyle, ...previous.categories[category], ...patch },
        },
      }
    })
  }

  return (
    <PreferenceSection title={t('emphasis.title')} description={t('emphasis.description')}>
      <PreferenceRow title={t('emphasis.enabledForAi')}>
        <Checkbox
          aria-label={t('emphasis.enabledForAi')}
          checked={settings.enabled_for_ai}
          onCheckedChange={(checked) =>
            save((previous) => ({ ...previous, enabled_for_ai: checked === true }))
          }
        />
      </PreferenceRow>
      <PreferenceRow title={t('emphasis.category')}>
        <NativeSelect
          aria-label={t('emphasis.category')}
          value={category}
          className='w-full'
          onChange={(event) => setCategory(event.target.value)}
        >
          <NativeSelectOption value='default'>{t('emphasis.default')}</NativeSelectOption>
          {LETTERING_CATEGORIES.map((value) => (
            <NativeSelectOption value={value} key={value}>
              {t(`emphasis.categories.${value}`)}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </PreferenceRow>
      <PreferenceRow title={t('emphasis.font')}>
        <FontPicker
          value={style.font ?? ''}
          families={fonts.data ?? []}
          ariaLabel={t('emphasis.font')}
          emptyLabel={inherit}
          onChange={(font) => change({ font: font || null })}
        />
      </PreferenceRow>
      <PreferenceRow title={t('emphasis.weight')}>
        <OptionalNumber
          key={`${category}-weight`}
          label={t('emphasis.weight')}
          value={style.font_weight ?? null}
          placeholder={inherit}
          min={1}
          max={1000}
          step={1}
          onCommit={(font_weight) => change({ font_weight })}
        />
      </PreferenceRow>
      <PreferenceRow title={t('emphasis.italic')}>
        <NativeSelect
          aria-label={t('emphasis.italic')}
          value={style.font_style ?? ''}
          className='w-full'
          onChange={(event) =>
            change({
              font_style:
                event.target.value === ''
                  ? null
                  : (event.target.value as 'italic' | 'normal' | 'oblique'),
            })
          }
        >
          <NativeSelectOption value=''>{inherit}</NativeSelectOption>
          <NativeSelectOption value='normal'>{t('emphasis.normal')}</NativeSelectOption>
          <NativeSelectOption value='italic'>{t('emphasis.italic')}</NativeSelectOption>
          <NativeSelectOption value='oblique'>{t('emphasis.oblique')}</NativeSelectOption>
        </NativeSelect>
      </PreferenceRow>
      <PreferenceRow title={t('emphasis.sizePercent')}>
        <OptionalNumber
          key={`${category}-size`}
          label={t('emphasis.sizePercent')}
          value={style.size_scale == null ? null : Math.round(style.size_scale * 10000) / 100}
          placeholder={inherit}
          min={10}
          max={1000}
          step={0.1}
          onCommit={(value) => change({ size_scale: value === null ? null : value / 100 })}
        />
      </PreferenceRow>
      {(['color', 'stroke_color', 'outer_stroke_color'] as const).map((field) => (
        <PreferenceRow key={field} title={t(`emphasis.${field}`)}>
          <div className='flex flex-wrap items-center gap-2'>
            <ColorWell
              value={style[field] ? rgbaToHex(style[field]) : null}
              label={t(`emphasis.${field}`)}
              allowTransparent
              emptyLabel={inherit}
              onChange={(color) => change({ [field]: color ? hexToRgba(color) : null })}
            />
            {style[field] == null && (
              <span className='text-[10px] text-muted-foreground'>{inherit}</span>
            )}
          </div>
        </PreferenceRow>
      ))}
      {(['stroke_width', 'outer_stroke_width'] as const).map((field) => (
        <PreferenceRow key={field} title={t(`emphasis.${field}`)}>
          <OptionalNumber
            key={`${category}-${field}`}
            label={t(`emphasis.${field}`)}
            value={style[field] ?? null}
            placeholder={inherit}
            min={0}
            max={100}
            step={0.1}
            onCommit={(value) => change({ [field]: value })}
          />
        </PreferenceRow>
      ))}
      {category !== 'default' && (
        <div className='py-3.5'>
          <Button
            variant='outline'
            size='sm'
            onClick={() =>
              save((previous) => {
                const categories = { ...previous.categories }
                delete categories[category]
                return { ...previous, categories }
              })
            }
          >
            {t('emphasis.resetCategory')}
          </Button>
        </div>
      )}
    </PreferenceSection>
  )
}

function OptionalNumber({
  label,
  value,
  placeholder,
  min,
  max,
  step,
  onCommit,
}: {
  label: string
  value: number | null
  placeholder: string
  min: number
  max: number
  step: number
  onCommit: (value: number | null) => void
}) {
  const [draft, setDraft] = useState(value === null ? '' : String(value))
  useEffect(() => setDraft(value === null ? '' : String(value)), [value])
  const commit = (draft: string) => {
    const next = draft.trim() === '' ? null : Number(draft)
    if (
      next !== null &&
      (!Number.isFinite(next) ||
        next < min ||
        next > max ||
        (step === 1 && !Number.isInteger(next)))
    ) {
      setDraft(value === null ? '' : String(value))
      return
    }
    if (next !== value) onCommit(next)
  }
  return (
    <Input
      type='number'
      aria-label={label}
      value={draft}
      placeholder={placeholder}
      min={min}
      max={max}
      step={step}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={(event) => commit(event.currentTarget.value)}
      onKeyDown={(event) => {
        if (event.key === 'Enter') event.currentTarget.blur()
        if (event.key === 'Escape') {
          setDraft(value === null ? '' : String(value))
          event.currentTarget.value = value === null ? '' : String(value)
          event.currentTarget.blur()
        }
      }}
    />
  )
}
