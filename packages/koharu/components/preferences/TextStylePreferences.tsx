'use client'

import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'

import { refreshTextStyles, saveTextStyles } from '@/lib/backend'
import { useKoharuStore } from '@/lib/store'
import { emptyTextStyle, styleGroups, styleSummary } from '@/lib/text-styles'
import { Button } from '@koharu/ui/components/button'

export function TextStylePreferences() {
  const { t } = useTranslation()
  const styles = useKoharuStore((state) => state.textStyles)
  useEffect(() => {
    if (!styles) void refreshTextStyles().catch(() => undefined)
  }, [styles])
  if (!styles) return <p role='status'>{t('common.loading')}</p>
  const defaultStyle = styles.default_style ?? emptyTextStyle
  return (
    <section className='mb-8' aria-labelledby='default-text-style'>
      <div className='mb-3 flex items-start justify-between gap-4'>
        <div>
          <h3 id='default-text-style' className='text-[12px] font-semibold'>
            {t('textStyles.defaultTitle')}
          </h3>
          <p className='mt-1 text-[11px] text-muted-foreground'>{t('textStyles.defaultHelp')}</p>
        </div>
        <Button
          size='sm'
          variant='outline'
          disabled={!styleGroups.some((group) => defaultStyle[group] != null)}
          onClick={() =>
            void saveTextStyles({ ...styles, default_style: { ...emptyTextStyle } }).catch(
              () => undefined,
            )
          }
        >
          {t('textStyles.clearAll')}
        </Button>
      </div>
      <ul className='divide-y rounded-xl border bg-[var(--surface-panel)]'>
        {styleGroups.map((group) => (
          <li key={group} className='flex min-w-0 items-center gap-3 px-4 py-2'>
            <div className='min-w-0 flex-1'>
              <p className='text-[11px] font-medium'>{t(`textStyles.groups.${group}`)}</p>
              <p className='mt-0.5 text-[11px] break-words text-muted-foreground'>
                {styleSummary(defaultStyle, group, t)}
              </p>
            </div>
            <Button
              size='sm'
              variant='ghost'
              disabled={defaultStyle[group] == null}
              aria-label={t('textStyles.clearGroup', { group: t(`textStyles.groups.${group}`) })}
              onClick={() =>
                void saveTextStyles({
                  ...styles,
                  default_style: { ...defaultStyle, [group]: null },
                }).catch(() => undefined)
              }
            >
              {t('textStyles.clear')}
            </Button>
          </li>
        ))}
      </ul>
    </section>
  )
}
