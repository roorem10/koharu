'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { FontPicker } from '@/components/controls/FontPicker'
import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { call, saveTextStyles } from '@/lib/backend'
import { pageKey, pagesKey, projectKey, refresh, useFonts, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { emptyShapeStyles, emptyTextStyle } from '@/lib/text-styles'
import { commands, type StyleTargetCounts } from '@koharu/bridge/protocol'
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

const shapes = ['round', 'rectangle', 'spiky', 'free'] as const

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
      <PreferenceSection title={t('textStyles.shapeTitle')} description={t('textStyles.shapeHelp')}>
        <PreferenceRow title={t('textStyles.shapeEnabled')}>
          <Switch
            checked={shapesStyle.enabled}
            aria-label={t('textStyles.shapeEnabled')}
            onCheckedChange={(enabled) =>
              void saveTextStyles({ ...styles, shape_styles: { ...shapesStyle, enabled } }).catch(
                () => undefined,
              )
            }
          />
        </PreferenceRow>
        {shapes.map((shape) => {
          const style = shapesStyle[shape] ?? emptyTextStyle
          return (
            <PreferenceRow key={shape} title={t(`textStyles.shapeFonts.${shape}`)}>
              <FontPicker
                value={style.font?.preferred_font ?? ''}
                families={fonts.data ?? []}
                ariaLabel={t(`textStyles.shapeFonts.${shape}`)}
                emptyLabel={t('textStyles.sameAsDefault')}
                onChange={(family) =>
                  void saveTextStyles({
                    ...styles,
                    shape_styles: {
                      ...shapesStyle,
                      [shape]: {
                        ...style,
                        font: family
                          ? { preferred_font: family, font_weight: null, font_style: null }
                          : null,
                      },
                    },
                  }).catch(() => undefined)
                }
              />
            </PreferenceRow>
          )
        })}
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
