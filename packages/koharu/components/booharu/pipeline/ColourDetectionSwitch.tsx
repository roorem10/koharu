'use client'

import { Trans, useTranslation } from 'react-i18next'

import { defaultTextStyleSection, useDetectColors } from '@/lib/booharu/lettering'
import { openSettingsSection } from '@/lib/booharu/settingsLink'
import { Switch } from '@koharu/ui/components/switch'

/** The colour detection switch, a row of Settings > Pipeline's Translator section: whether newly
 * translated text keeps the colours of the original lettering, `detect_colors` of
 * `[booharu-lettering]`, saved at once, with a note linking to the default text style, whose
 * colours it gets when the switch is off. Laid out as Koharu's preference rows are, with the note
 * under the description. */
export function ColourDetectionSwitch() {
  const { t, i18n } = useTranslation()
  const { detect, loaded, set } = useDetectColors()
  const title = t('booharu.pipeline.detectColors')

  return (
    <div className='grid grid-cols-[minmax(150px,0.9fr)_minmax(220px,1.1fr)] items-center gap-6 py-3.5'>
      <div>
        <p className='text-[12px] font-medium'>{title}</p>
        <p className='mt-0.5 text-[10px] leading-4 text-muted-foreground'>
          {t('booharu.pipeline.detectColorsDescription')}
        </p>
        <p className='mt-1 text-[10px] leading-4 text-muted-foreground'>
          <Trans
            t={t}
            i18n={i18n}
            i18nKey='booharu.pipeline.detectColorsNote'
            values={{ tab: t('settings.tabs.typesetting') }}
            components={{
              a: (
                <button
                  type='button'
                  className='text-primary underline underline-offset-2 hover:text-foreground'
                  onClick={() => openSettingsSection('typesetting', defaultTextStyleSection)}
                />
              ),
            }}
          />
        </p>
      </div>
      <div className='min-w-0'>
        <Switch
          aria-label={title}
          checked={detect}
          disabled={!loaded}
          onCheckedChange={(next) => set(next)}
        />
      </div>
    </div>
  )
}
