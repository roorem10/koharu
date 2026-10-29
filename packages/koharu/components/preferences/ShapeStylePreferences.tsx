'use client'

import { useTranslation } from 'react-i18next'

import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { saveTextStyles } from '@/lib/backend'
import { useKoharuStore } from '@/lib/store'
import { styleGroups } from '@/lib/text-styles'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'
import { Switch } from '@koharu/ui/components/switch'

const shapes = ['rectangle', 'round', 'spiky', 'free'] as const

export function ShapeStylePreferences() {
  const { t } = useTranslation()
  const styles = useKoharuStore((state) => state.textStyles)
  if (!styles) return null
  const rules = styles.shape_rules ?? {
    enabled: false,
    rectangle: null,
    round: null,
    spiky: null,
    free: null,
  }
  const presets = styles.presets ?? []
  return (
    <div className='mb-8'>
      <PreferenceSection title={t('textStyles.shapeTitle')}>
        <PreferenceRow title={t('textStyles.shapeEnabled')} description={t('textStyles.shapeHelp')}>
          <Switch
            checked={!!rules.enabled}
            aria-label={t('textStyles.shapeTitle')}
            onCheckedChange={(enabled) =>
              void saveTextStyles({ ...styles, shape_rules: { ...rules, enabled } }).catch(
                () => undefined,
              )
            }
          />
        </PreferenceRow>
        {shapes.map((shape) => {
          const id = rules[shape] ?? ''
          const missing = !!id && !presets.some((preset) => preset.id === id)
          const items = {
            '': t('textStyles.defaultOnly'),
            ...Object.fromEntries(presets.map((preset) => [preset.id, preset.name])),
            ...(missing ? { [id]: t('textStyles.presetMissing') } : {}),
          }
          return (
            <PreferenceRow
              key={shape}
              title={t(`textStyles.shapes.${shape}`)}
              description={t(`textStyles.shapeHints.${shape}`)}
            >
              <div className='grid w-64 max-w-full gap-1'>
                <Select
                  value={id}
                  items={items}
                  onValueChange={(id) => {
                    if (id !== null)
                      void saveTextStyles({
                        ...styles,
                        shape_rules: { ...rules, [shape]: id || null },
                      }).catch(() => undefined)
                  }}
                >
                  <SelectTrigger
                    className='h-8 w-full min-w-0 text-[11px]'
                    aria-label={t(`textStyles.shapes.${shape}`)}
                    title={items[id]}
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value=''>{t('textStyles.defaultOnly')}</SelectItem>
                    {presets.map((preset) => (
                      <SelectItem key={preset.id} value={preset.id}>
                        <span className='min-w-0'>
                          <span className='block break-words'>{preset.name}</span>
                          <span className='block text-[10px] text-muted-foreground'>
                            {styleGroups
                              .filter((group) => preset.style[group] != null)
                              .map((group) => t(`textStyles.groups.${group}`))
                              .join(' · ')}
                          </span>
                        </span>
                      </SelectItem>
                    ))}
                    {missing && (
                      <SelectItem value={id} disabled>
                        {t('textStyles.presetMissing')}
                      </SelectItem>
                    )}
                  </SelectContent>
                </Select>
              </div>
            </PreferenceRow>
          )
        })}
      </PreferenceSection>
    </div>
  )
}
