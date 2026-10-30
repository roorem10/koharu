'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { FontPicker } from '@/components/controls/FontPicker'
import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { call, saveTextStyles } from '@/lib/backend'
import { LETTERING_CATEGORIES } from '@/lib/line-labels'
import { pageKey, pagesKey, projectKey, refresh, useFonts, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  emptyShapeStyles,
  categoryShapes,
  categoryStyle,
  withCategoryStyle,
  isEmptyStyle,
  suggestCategoryFonts,
} from '@/lib/text-styles'
import { commands, type StyleTargetCounts, type FontPart } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@koharu/ui/components/dialog'
import { Switch } from '@koharu/ui/components/switch'
import { toast } from '@koharu/ui/components/toast'

export function ShapeStylePreferences() {
  const { t } = useTranslation()
  const styles = useKoharuStore((state) => state.textStyles)
  const fonts = useFonts()
  const project = useProject().data
  const [counts, setCounts] = useState<StyleTargetCounts | null>(null)
  const [includeUser, setIncludeUser] = useState(false)
  const [busy, setBusy] = useState(false)
  if (!styles) return null
  const shapesStyle = styles.shape_styles ?? emptyShapeStyles
  const confirm = async () => {
    setBusy(true)
    try {
      const counts = await call(commands.countTextStyleTargets, 'project')
      setIncludeUser(false)
      setCounts(counts)
    } finally {
      setBusy(false)
    }
  }
  const apply = async () => {
    setBusy(true)
    try {
      const result = await call(commands.applyShapeRules, 'project', includeUser)
      await refresh(projectKey, pagesKey, pageKey)
      toast.add({
        type: 'success',
        title: t('textStyles.applied', { count: result.changed }),
        description:
          [
            result.skipped ? t('textStyles.skipped', { count: result.skipped }) : '',
            result.kept_manual ? t('textStyles.keptManual', { count: result.kept_manual }) : '',
          ]
            .filter(Boolean)
            .join(' ') || undefined,
      })
      setCounts(null)
    } finally {
      setBusy(false)
    }
  }
  return (
    <>
      <PreferenceSection title={t('lettering.title')} description={t('lettering.description')}>
        <PreferenceRow title={t('lettering.enabled')}>
          <Switch
            checked={shapesStyle.enabled ?? false}
            aria-label={t('lettering.enabled')}
            onCheckedChange={(enabled) =>
              void saveTextStyles({ ...styles, shape_styles: { ...shapesStyle, enabled } }).catch(
                () => undefined,
              )
            }
          />
        </PreferenceRow>
        {LETTERING_CATEGORIES.map((category) => {
          const style = categoryStyle(styles, category)
          const name = t(`lettering.categories.${category}`)
          const shape = categoryShapes[category]
          const empty = isEmptyStyle(style)
          const font = style.font ?? shapesStyle.round?.font ?? styles.default_style?.font
          const updateFont = (part: Partial<FontPart>) => {
            void saveTextStyles(
              withCategoryStyle(styles, category, {
                ...style,
                font: {
                  preferred_font: null,
                  font_weight: null,
                  font_style: null,
                  ...font,
                  ...part,
                },
              }),
            ).catch(() => undefined)
          }
          return (
            <PreferenceRow
              key={category}
              title={name}
              description={[
                shape ? t(`lettering.shapeHelp.${shape}`) : '',
                empty
                  ? t(
                      category === 'normal_dialogue'
                        ? 'lettering.sameAsDefault'
                        : 'lettering.sameAsDialogue',
                    )
                  : '',
              ]
                .filter(Boolean)
                .join(' ')}
            >
              <div
                className='flex min-w-0 flex-wrap items-center gap-2'
                data-testid={`lettering-row-${category}`}
              >
                <div className='min-w-[120px] flex-1'>
                  <FontPicker
                    value={style.font?.preferred_font ?? ''}
                    families={fonts.data ?? []}
                    ariaLabel={name}
                    emptyLabel={t(
                      category === 'normal_dialogue'
                        ? 'lettering.sameAsDefault'
                        : 'lettering.sameAsDialogue',
                    )}
                    onChange={(family) =>
                      void saveTextStyles(
                        withCategoryStyle(styles, category, {
                          ...style,
                          font: family
                            ? {
                                preferred_font: family,
                                font_weight: style.font?.font_weight ?? null,
                                font_style: style.font?.font_style ?? null,
                              }
                            : null,
                        }),
                      ).catch(() => undefined)
                    }
                  />
                </div>
                <label className='flex items-center gap-1 text-[10px]'>
                  <Checkbox
                    aria-label={t('lettering.italicFor', { category: name })}
                    checked={font?.font_style === 'italic'}
                    onCheckedChange={(checked) =>
                      updateFont({ font_style: checked === true ? 'italic' : 'normal' })
                    }
                  />
                  {t('lettering.italic')}
                </label>
                <label className='flex items-center gap-1 text-[10px]'>
                  <Checkbox
                    aria-label={t('lettering.boldFor', { category: name })}
                    checked={(font?.font_weight ?? 400) >= 700}
                    onCheckedChange={(checked) =>
                      updateFont({ font_weight: checked === true ? 700 : 400 })
                    }
                  />
                  {t('lettering.bold')}
                </label>
              </div>
            </PreferenceRow>
          )
        })}
        <PreferenceRow title={t('lettering.suggestFonts')} description={t('lettering.suggestHelp')}>
          <Button
            size='sm'
            variant='outline'
            disabled={!fonts.data?.length}
            onClick={() => {
              const next = suggestCategoryFonts(styles, fonts.data ?? [])
              if (next !== styles) void saveTextStyles(next).catch(() => undefined)
            }}
          >
            {t('lettering.suggestFonts')}
          </Button>
        </PreferenceRow>
        <div className='py-3.5'>
          <Button
            size='sm'
            variant='outline'
            disabled={!project || !shapesStyle.enabled || busy}
            onClick={() => void confirm().catch(() => undefined)}
          >
            {t('textStyles.applyOpenProject')}
          </Button>
        </div>
      </PreferenceSection>
      <Dialog
        open={counts !== null}
        onOpenChange={(open) => {
          if (!open && !busy) setCounts(null)
        }}
      >
        <DialogContent showCloseButton={false}>
          <DialogTitle>{t('textStyles.applyOpenProject')}</DialogTitle>
          <DialogDescription>
            {t('textStyles.confirmApply', {
              count: counts ? counts.restylable + (includeUser ? counts.manual : 0) : 0,
            })}
          </DialogDescription>
          <p className='text-sm text-muted-foreground'>
            {t('textStyles.manualCount', { count: counts?.manual ?? 0 })}
          </p>
          <label className='flex items-center gap-2 text-sm'>
            <Checkbox
              checked={includeUser}
              onCheckedChange={(checked) => setIncludeUser(checked === true)}
            />
            {t('textStyles.includeManual')}
          </label>
          <DialogFooter>
            <Button variant='outline' disabled={busy} onClick={() => setCounts(null)}>
              {t('textStyles.cancel')}
            </Button>
            <Button
              disabled={busy || !counts || !(counts.restylable + (includeUser ? counts.manual : 0))}
              onClick={() => void apply().catch(() => undefined)}
            >
              {t('textStyles.apply')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
