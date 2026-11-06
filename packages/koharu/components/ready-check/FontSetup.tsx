'use client'

import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { rankCategoryFonts, type FontMatch } from '@/components/ready-check/font-ranking'
import { refreshTextStyles, saveTextStyles } from '@/lib/backend'
import { LETTERING_CATEGORIES } from '@/lib/line-labels'
import { useFonts } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { categoryStyle, withCategoryStyle } from '@/lib/text-styles'
import type { LetteringCategory } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

const matchKeys: Record<FontMatch, string> = {
  comic: 'readyCheck.fontSetup.matchComic',
  handwriting: 'readyCheck.fontSetup.matchHandwriting',
  bold: 'readyCheck.fontSetup.matchBold',
  display: 'readyCheck.fontSetup.matchDisplay',
  plain: 'readyCheck.fontSetup.matchPlain',
  italic: 'readyCheck.fontSetup.matchItalic',
  category: 'readyCheck.fontSetup.matchCategory',
}

export function FontSetup({ onSaved }: { onSaved?: () => void | Promise<void> }) {
  const { t } = useTranslation()
  const styles = useKoharuStore((state) => state.textStyles)
  const fonts = useFonts()
  const [saving, setSaving] = useState(false)
  const suggestions = useMemo(
    () =>
      LETTERING_CATEGORIES.map((category) => ({
        category,
        candidates: rankCategoryFonts(category, fonts.data ?? []).slice(0, 3),
      })),
    [fonts.data],
  )

  useEffect(() => {
    if (!styles) void refreshTextStyles().catch(() => undefined)
  }, [styles])

  const accept = async (category: LetteringCategory, family: string) => {
    const current = useKoharuStore.getState().textStyles
    if (!current) return
    const style = categoryStyle(current, category)
    setSaving(true)
    try {
      await saveTextStyles(
        withCategoryStyle(current, category, {
          ...style,
          font: {
            preferred_font: family,
            font_weight: style.font?.font_weight ?? null,
            font_style: style.font?.font_style ?? null,
          },
        }),
      )
      await onSaved?.()
    } finally {
      setSaving(false)
    }
  }

  return (
    <section className='grid gap-3'>
      <div>
        <h3 className='text-sm font-semibold'>{t('readyCheck.fontSetup.title')}</h3>
        <p className='mt-1 text-xs text-muted-foreground'>
          {t('readyCheck.fontSetup.description')}
        </p>
      </div>
      {fonts.isError ? (
        <p role='alert'>{t('settings.typesetting.loadError')}</p>
      ) : !styles || fonts.isPending ? (
        <p role='status'>{t('common.loading')}</p>
      ) : (
        <div className='divide-y rounded-xl border px-4'>
          {suggestions.map(({ category, candidates }) => {
            const name = t(`lettering.categories.${category}`)
            const current = categoryStyle(styles, category).font?.preferred_font
            return (
              <div
                key={category}
                className='grid gap-2 py-3'
                data-testid={`font-setup-${category}`}
              >
                <div className='flex flex-wrap items-baseline justify-between gap-2'>
                  <h4 className='text-xs font-medium'>{name}</h4>
                  <p className='text-xs break-words text-muted-foreground'>
                    {current
                      ? t('readyCheck.fontSetup.current', { font: current })
                      : t(
                          category === 'normal_dialogue'
                            ? 'lettering.sameAsDefault'
                            : 'lettering.sameAsDialogue',
                        )}
                  </p>
                </div>
                {candidates.length === 0 ? (
                  <p className='text-xs text-muted-foreground'>
                    {t('readyCheck.fontSetup.noMatch')}
                  </p>
                ) : (
                  <div className='grid gap-2 sm:grid-cols-3'>
                    {candidates.map(({ family, match }) => (
                      <Button
                        key={family.name}
                        type='button'
                        variant='outline'
                        className='h-auto min-w-0 flex-col items-start gap-1 p-3 text-left whitespace-normal'
                        aria-label={t('readyCheck.fontSetup.useFont', {
                          font: family.name,
                          category: name,
                        })}
                        aria-pressed={current === family.name}
                        disabled={saving || current === family.name}
                        onClick={() => void accept(category, family.name).catch(() => undefined)}
                      >
                        <span className='w-full text-xs break-words'>{family.name}</span>
                        <span className='text-[10px] font-normal text-muted-foreground'>
                          {t(matchKeys[match])}
                        </span>
                        <span
                          className='w-full text-base font-normal break-words'
                          style={{ fontFamily: JSON.stringify(family.name) }}
                        >
                          {t('readyCheck.fontSetup.preview')}
                        </span>
                      </Button>
                    ))}
                  </div>
                )}
              </div>
            )
          })}
        </div>
      )}
    </section>
  )
}
