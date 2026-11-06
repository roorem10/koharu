'use client'

import { useTranslation } from 'react-i18next'

import { ColorWell } from '@/components/controls/ColorWell'
import {
  useCanvasBackgrounds,
  useSaveCanvasBackground,
} from '@/components/preferences/canvas-background'
import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { hexToRgba, rgbaToHex } from '@/lib/color'
import { Button } from '@koharu/ui/components/button'

export function CanvasBackgroundPreferences() {
  const { t } = useTranslation()
  const backgrounds = useCanvasBackgrounds()
  const save = useSaveCanvasBackground()
  const colors = backgrounds.data

  return (
    <PreferenceSection
      title={t('settings.appearance.canvasBackground')}
      description={t('settings.appearance.canvasBackgroundDescription')}
    >
      {!colors ? (
        <div className='py-3 text-xs' role={backgrounds.isError ? 'alert' : 'status'}>
          <p>
            {t(
              backgrounds.isError
                ? 'settings.appearance.canvasBackgroundLoadError'
                : 'settings.loading',
            )}
          </p>
          {backgrounds.isError && (
            <Button
              size='sm'
              variant='outline'
              className='mt-2'
              onClick={() => void backgrounds.refetch()}
            >
              {t('errors.tryAgain')}
            </Button>
          )}
        </div>
      ) : (
        (['light', 'dark'] as const).map((theme) => {
          const color = colors[theme]
          if (!color) return null
          const label = t(`settings.appearance.${theme}CanvasBackground`)
          return (
            <PreferenceRow key={theme} title={label}>
              <ColorWell
                label={label}
                value={rgbaToHex([...color, 255])}
                disabled={save.isPending}
                onChange={(value) => {
                  const [red, green, blue] = hexToRgba(value)
                  save.mutate({ theme, color: [red, green, blue] })
                }}
              />
            </PreferenceRow>
          )
        })
      )}
    </PreferenceSection>
  )
}
