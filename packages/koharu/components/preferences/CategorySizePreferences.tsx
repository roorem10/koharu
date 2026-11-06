'use client'

import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useLetteringSettings, useSaveLetteringSettings } from '@/components/lettering/queries'
import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { LETTERING_CATEGORIES } from '@/lib/line-labels'
import { useKoharuStore } from '@/lib/store'
import { categoryStyle } from '@/lib/text-styles'
import type { CategorySize, LetteringCategory } from '@koharu/bridge/protocol'
import { Input } from '@koharu/ui/components/input'
import { NativeSelect, NativeSelectOption } from '@koharu/ui/components/native-select'
import { Switch } from '@koharu/ui/components/switch'

export function CategorySizePreferences() {
  const { t } = useTranslation()
  const settings = useLetteringSettings()
  const save = useSaveLetteringSettings()
  const styles = useKoharuStore((state) => state.textStyles)
  const [category, setCategory] = useState<LetteringCategory>('normal_dialogue')
  const size = settings.data?.category_sizes?.[category]
  const percent = size?.base_size == null ? null : Math.round(size.base_size * 10000) / 100
  const [draft, setDraft] = useState('')
  useEffect(() => setDraft(percent === null ? '' : String(percent)), [category, percent])

  const update = (patch: Partial<CategorySize>) => {
    if (!settings.data || save.isPending) return
    save.mutate(
      {
        ...settings.data,
        category_sizes: {
          ...settings.data.category_sizes,
          [category]: { base_size: null, may_grow: false, ...size, ...patch },
        },
      },
      { onError: () => setDraft(percent === null ? '' : String(percent)) },
    )
  }
  const commit = (value: string) => {
    const next = value.trim() === '' ? null : Number(value)
    if (next !== null && (!Number.isFinite(next) || next <= 0 || next > 100)) {
      setDraft(percent === null ? '' : String(percent))
      return
    }
    if (next !== percent) update({ base_size: next === null ? null : next / 100 })
  }
  const font =
    styles &&
    (categoryStyle(styles, category).font ??
      styles.shape_styles?.round?.font ??
      styles.default_style?.font)
  const previewSize = size?.base_size == null ? null : Math.round(size.base_size * 1000 * 10) / 10

  return (
    <PreferenceSection
      title={t('sizeConsistency.title')}
      description={t('sizeConsistency.settingsHelp')}
    >
      <PreferenceRow title={t('sizeConsistency.category')}>
        <NativeSelect
          aria-label={t('sizeConsistency.category')}
          value={category}
          className='w-full'
          disabled={!settings.data || save.isPending}
          onChange={(event) => setCategory(event.target.value as LetteringCategory)}
        >
          {LETTERING_CATEGORIES.map((value) => (
            <NativeSelectOption value={value} key={value}>
              {t(`lettering.categories.${value}`)}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </PreferenceRow>
      <PreferenceRow
        title={t('sizeConsistency.baseSize')}
        description={t('sizeConsistency.baseHelp')}
      >
        <Input
          type='number'
          aria-label={t('sizeConsistency.baseSize')}
          value={draft}
          placeholder={t('sizeConsistency.uncapped')}
          min={0.01}
          max={100}
          step={0.1}
          disabled={!settings.data || save.isPending}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={(event) => commit(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') event.currentTarget.blur()
            if (event.key === 'Escape') {
              const previous = percent === null ? '' : String(percent)
              setDraft(previous)
              event.currentTarget.value = previous
              event.currentTarget.blur()
            }
          }}
        />
      </PreferenceRow>
      <PreferenceRow title={t('sizeConsistency.mayGrow')}>
        <Switch
          aria-label={t('sizeConsistency.mayGrow')}
          checked={size?.may_grow ?? false}
          disabled={!settings.data || save.isPending || percent === null}
          onCheckedChange={(may_grow) => update({ may_grow })}
        />
      </PreferenceRow>
      <div className='py-4'>
        <p className='mb-2 text-[10px] text-muted-foreground'>
          {previewSize === null
            ? t('sizeConsistency.uncapped')
            : t('sizeConsistency.previewScale', { size: previewSize, height: 1000 })}
        </p>
        <div className='max-h-40 overflow-auto rounded-lg border bg-background px-4 py-3'>
          <p
            style={{
              fontFamily: font?.preferred_font ?? undefined,
              fontWeight: font?.font_weight ?? undefined,
              fontStyle: font?.font_style ?? undefined,
              fontSize: previewSize ?? 18,
            }}
          >
            {t('sizeConsistency.preview')}
          </p>
        </div>
        {(settings.isPending || settings.isError) && (
          <p role='status' className='mt-2 text-[10px] text-muted-foreground'>
            {t(settings.isError ? 'lettering.loadError' : 'lettering.loading')}
          </p>
        )}
      </div>
    </PreferenceSection>
  )
}
