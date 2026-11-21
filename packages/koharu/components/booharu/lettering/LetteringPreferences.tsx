'use client'

import { Bold, Italic, Minus, Plus } from 'lucide-react'
import { useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { CategoryPicker } from '@/components/booharu/lettering/CategoryPicker'
import { ApplyStyles } from '@/components/booharu/typesetting/ApplyStyles'
import { ColorWell } from '@/components/controls/ColorWell'
import { FontPicker } from '@/components/controls/FontPicker'
import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import {
  categoryName,
  letteredFont,
  letteringCategories,
  letteringCategoriesSection,
  setAuto,
  setFamily,
  setStyle,
  shapeNote,
  shownFamily,
  suggestCategoryFonts,
  suggestFonts,
  useCategorySettings,
} from '@/lib/booharu/categories'
import { useBooharuFontPicker } from '@/lib/booharu/fonts'
import {
  builtInSubtitle,
  SUBTITLE_PERCENT,
  subtitlePercent,
  subtitleShare,
  useSubtitleLook,
} from '@/lib/booharu/sfx'
import { toHex, toRgba } from '@/lib/color'
import { useFonts } from '@/lib/queries'
import type { CategoryFont, LetteringCategory } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  NumberField,
  NumberFieldDecrement,
  NumberFieldGroup,
  NumberFieldIncrement,
  NumberFieldInput,
} from '@koharu/ui/components/number-field'
import { Switch } from '@koharu/ui/components/switch'
import { toast } from '@koharu/ui/components/toast'
import { Toggle } from '@koharu/ui/components/toggle'

/** `names` joined as `language` joins a list, Wu, which Intl may lack, as Mandarin does. */
function joinNames(names: string[], language: string): string {
  const locales = language === 'wuu' ? ['wuu', 'zh-CN'] : [language]
  return new Intl.ListFormat(locales, { type: 'conjunction' }).format(names)
}

/** Settings > Typesetting's Lettering categories, below the default text style:
 * `[booharu-lettering]` `auto`, the Pick fonts automatically switch, and `categories`, each
 * category's own font. A searchable picker chooses the category whose font, bold and italic show
 * under it, the four a bubble's shape gives saying which bubbles they letter too; the categories
 * with a font of their own are listed, each a button that chooses it; Suggest fonts fills the
 * categories with none from 0.0.4's starting list; and Apply gives the open project's translations
 * their category fonts. Each change saves at once. The Sound effect subtitles card follows it. */
export function LetteringPreferences() {
  const { t, i18n } = useTranslation()
  const { settings, change } = useCategorySettings()
  const fonts = useFonts()
  const booharuFonts = useBooharuFontPicker()
  const [category, setCategory] = useState<LetteringCategory>('normal_dialogue')
  const titleId = useId()
  const loaded = settings !== undefined
  const categories = settings?.categories ?? {}
  const families = fonts.data ?? []
  const name = t(categoryName(category))
  const font = letteredFont(categories, category)
  const note = shapeNote(category)
  const own = letteringCategories.flatMap((candidate) => {
    const entry = categories[candidate]
    return entry ? [{ category: candidate, entry }] : []
  })

  /** An own font as the list shows it: its family, or for none Normal Dialogue's, or Normal
   * Dialogue's the default font, bold, italic. */
  const describe = (candidate: LetteringCategory, entry: CategoryFont) =>
    [
      entry.family ??
        t(
          candidate === 'normal_dialogue'
            ? 'booharu.lettering.defaultFont'
            : 'booharu.lettering.sameAsDialogue',
        ),
      entry.bold && t('booharu.lettering.bold'),
      entry.italic && t('booharu.lettering.italic'),
    ]
      .filter(Boolean)
      .join(' · ')

  const suggest = () => {
    if (!settings) return
    // What the file holds just before the save decides; this, what shows, is what it says.
    const { filled } = suggestCategoryFonts(settings.categories, families)
    if (!filled.length) {
      toast.add({ type: 'info', title: t('booharu.lettering.suggestedNone') })
      return
    }
    void change(suggestFonts(families)).then((saved) => {
      if (!saved) return
      toast.add({
        type: 'success',
        title: t('booharu.lettering.suggested', { count: filled.length }),
        description: joinNames(
          filled.map((filledCategory) => t(categoryName(filledCategory))),
          i18n.language,
        ),
      })
    })
  }

  const style = (key: 'bold' | 'italic', Icon: typeof Bold) => (
    <Toggle
      variant='outline'
      size='sm'
      aria-label={t(`booharu.lettering.${key}For`, { category: name })}
      pressed={font[key]}
      disabled={!loaded}
      onPressedChange={(on) => void change(setStyle(category, key, on))}
    >
      <Icon />
      {t(`booharu.lettering.${key}`)}
    </Toggle>
  )

  return (
    <>
      {/* Focusable, so a link can take the focus to it. */}
      <section
        id={letteringCategoriesSection}
        tabIndex={-1}
        aria-labelledby={titleId}
        className='outline-none'
      >
        <header className='mb-3'>
          <h3 id={titleId} className='text-[12px] font-semibold'>
            {t('booharu.lettering.title')}
          </h3>
          <p className='mt-0.5 text-[11px] leading-4 text-muted-foreground'>
            {t('booharu.lettering.description')}
          </p>
        </header>
        <div className='divide-y divide-border rounded-xl border border-border/80 bg-[var(--surface-panel)] px-5'>
          <PreferenceRow
            title={t('booharu.lettering.auto')}
            description={t('booharu.lettering.autoDescription')}
          >
            <Switch
              aria-label={t('booharu.lettering.auto')}
              checked={settings?.auto ?? false}
              disabled={!loaded}
              onCheckedChange={(on) => void change(setAuto(on))}
            />
          </PreferenceRow>
          <PreferenceRow title={t('booharu.lettering.category')}>
            <CategoryPicker
              value={category}
              label={t('booharu.lettering.category')}
              onChange={setCategory}
            />
          </PreferenceRow>
          <PreferenceRow
            title={t('booharu.lettering.font')}
            description={note ? t(note) : undefined}
            align='start'
          >
            <div className='grid gap-2'>
              <FontPicker
                value={shownFamily(categories, category)}
                families={families}
                disabled={!loaded}
                ariaLabel={t('booharu.lettering.fontFor', { category: name })}
                onChange={(family) => void change(setFamily(category, family))}
                {...booharuFonts}
                noneLabel={t(
                  category === 'normal_dialogue'
                    ? 'booharu.lettering.sameAsDefault'
                    : 'booharu.lettering.sameAsDialogue',
                )}
              />
              <div className='flex flex-wrap gap-1.5'>
                {style('bold', Bold)}
                {style('italic', Italic)}
              </div>
            </div>
          </PreferenceRow>
          <PreferenceRow title={t('booharu.lettering.own')} align='start'>
            {own.length ? (
              <ul aria-label={t('booharu.lettering.own')} className='flex flex-wrap gap-1.5'>
                {own.map(({ category: candidate, entry }) => (
                  <li key={candidate} className='min-w-0'>
                    <Button
                      type='button'
                      variant='outline'
                      size='xs'
                      aria-pressed={candidate === category}
                      className='max-w-full font-normal aria-pressed:bg-accent'
                      onClick={() => setCategory(candidate)}
                    >
                      <span className='truncate'>{t(categoryName(candidate))}</span>{' '}
                      <span className='truncate text-muted-foreground'>
                        {describe(candidate, entry)}
                      </span>
                    </Button>
                  </li>
                ))}
              </ul>
            ) : (
              <p className='text-[11px] leading-4 text-muted-foreground'>
                {t('booharu.lettering.ownNone')}
              </p>
            )}
          </PreferenceRow>
          <PreferenceRow
            title={t('booharu.lettering.suggest')}
            description={t('booharu.lettering.suggestDescription')}
          >
            <Button
              type='button'
              size='sm'
              variant='outline'
              disabled={!loaded || !families.length}
              onClick={suggest}
            >
              {t('booharu.lettering.suggest')}
            </Button>
          </PreferenceRow>
          <ApplyStyles categories />
        </div>
      </section>
      <SubtitlePreferences />
    </>
  )
}

/** The Sound effect subtitles card, after Lettering categories: `subtitle` of `[booharu-lettering]`,
 * the look of the small translation a sound effect kept as art can get beside it. Its font, the
 * default font while none is chosen; its size, as a percentage of the page's shorter side, which
 * the field keeps within 0.1% to 10% and saves once it commits it, as it loses focus or a step
 * ends, a cleared field saving nothing and showing the saved size again; and its text and outline
 * colours, the outline as wide as a ninth of the size. Every other change saves at once. */
function SubtitlePreferences() {
  const { t } = useTranslation()
  const { look, change } = useSubtitleLook()
  const fonts = useFonts()
  const booharuFonts = useBooharuFontPicker()
  // The size while the field is edited; undefined while it isn't.
  const [editing, setEditing] = useState<number | null | undefined>(undefined)
  const loaded = look !== undefined
  const shown = look ?? builtInSubtitle
  const size = look ? subtitlePercent(look.size) : null
  const fill = toHex(shown.fill)
  const stroke = toHex(shown.stroke)

  return (
    <PreferenceSection
      title={t('booharu.sfx.subtitle.title')}
      description={t('booharu.sfx.subtitle.description')}
    >
      <PreferenceRow title={t('booharu.lettering.font')}>
        <FontPicker
          value={shown.family ?? ''}
          families={fonts.data ?? []}
          disabled={!loaded}
          ariaLabel={t('booharu.sfx.subtitle.fontLabel')}
          // The leading row stands for no family: the default font.
          onChange={(family) => change({ family: family || null })}
          {...booharuFonts}
          noneLabel={t('booharu.lettering.defaultFont')}
        />
      </PreferenceRow>
      <PreferenceRow
        title={t('booharu.sfx.subtitle.size')}
        description={t('booharu.sfx.subtitle.sizeHint')}
      >
        <div className='flex items-center gap-1.5'>
          <NumberField
            className='w-28'
            value={editing === undefined ? size : editing}
            min={SUBTITLE_PERCENT.min}
            max={SUBTITLE_PERCENT.max}
            step={SUBTITLE_PERCENT.step}
            smallStep={SUBTITLE_PERCENT.step}
            format={{ maximumFractionDigits: 2 }}
            disabled={!loaded}
            onValueChange={setEditing}
            onValueCommitted={(percent) => {
              setEditing(undefined)
              const share = subtitleShare(percent)
              if (share === null || percent === size) return
              change({ size: share })
            }}
          >
            <NumberFieldGroup>
              <NumberFieldDecrement aria-label={t('booharu.sfx.subtitle.smaller')}>
                <Minus />
              </NumberFieldDecrement>
              <NumberFieldInput aria-label={t('booharu.sfx.subtitle.sizeLabel')} />
              <NumberFieldIncrement aria-label={t('booharu.sfx.subtitle.larger')}>
                <Plus />
              </NumberFieldIncrement>
            </NumberFieldGroup>
          </NumberField>
          <span aria-hidden='true' className='text-[11px] text-muted-foreground'>
            %
          </span>
        </div>
      </PreferenceRow>
      <PreferenceRow title={t('booharu.typesetting.fill')}>
        <div className='flex min-w-0 items-center gap-2'>
          <ColorWell
            label={t('booharu.sfx.subtitle.fillLabel')}
            size='sm'
            disabled={!loaded}
            value={fill}
            onChange={(color) => color !== fill && change({ fill: toRgba(color) })}
          />
          <span className='min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground'>
            {fill}
          </span>
        </div>
      </PreferenceRow>
      <PreferenceRow
        title={t('booharu.typesetting.outlineColor')}
        description={t('booharu.sfx.subtitle.outlineHint')}
      >
        <div className='flex min-w-0 items-center gap-2'>
          <ColorWell
            label={t('booharu.sfx.subtitle.outlineLabel')}
            size='sm'
            disabled={!loaded}
            value={stroke}
            onChange={(color) => color !== stroke && change({ stroke: toRgba(color) })}
          />
          <span className='min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground'>
            {stroke}
          </span>
        </div>
      </PreferenceRow>
    </PreferenceSection>
  )
}
