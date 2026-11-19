'use client'

import { useTranslation } from 'react-i18next'

import { useReadChapter } from '@/lib/booharu/readChapter'
import { applyPreset, presetLabel, sameStages, useRunPresets, withRead } from '@/lib/booharu/runs'
import { useKoharuStore } from '@/lib/store'
import { Button } from '@koharu/ui/components/button'

/** The run presets in the Run menu's root view, a button each: one switches on its stages and the
 * others off, Read chapter too, and starts nothing, so Run starts them. The preset whose stages are
 * the ones on, Read chapter's included, shows pressed. Nothing shows until the presets are read, or
 * while there are none. */
export function PresetButtons() {
  const { t } = useTranslation()
  const presets = useRunPresets()
  const processing = useKoharuStore((state) => state.processingStages)
  const read = useReadChapter().on
  const stages = read ? withRead(processing) : processing
  if (!presets?.length) return null
  const title = t('booharu.runs.presets.short')

  return (
    <div role='group' aria-label={title} className='grid min-w-0 gap-1 px-2 pt-1 pb-1.5'>
      <span aria-hidden='true' className='text-[11px]'>
        {title}
      </span>
      <div className='flex max-h-24 min-w-0 flex-wrap gap-1 overflow-y-auto'>
        {presets.map((preset) => {
          const label = presetLabel(preset, t)
          return (
            <Button
              key={preset.name}
              type='button'
              size='xs'
              variant='outline'
              aria-pressed={sameStages(preset.stages, stages)}
              title={label}
              className='h-6 max-w-full min-w-0 rounded-md px-2 text-[10px] font-normal text-muted-foreground aria-pressed:border-primary/60 aria-pressed:bg-primary/10 aria-pressed:text-foreground'
              onClick={() => applyPreset(preset)}
            >
              <span className='truncate'>{label}</span>
            </Button>
          )
        })}
      </div>
    </div>
  )
}
