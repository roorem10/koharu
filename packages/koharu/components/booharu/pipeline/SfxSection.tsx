'use client'

import { useId } from 'react'
import { useTranslation } from 'react-i18next'

import { sfxModes, useSfxMode } from '@/lib/booharu/sfx'
import type { SfxMode } from '@koharu/ipc/protocol'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

/** The sound effects row of Settings > Pipeline's Translator section: which sound effects a run
 * translates, `sfx` of `[booharu-lettering]`, saved at once. Each mode in the list says what it
 * does, the chosen one's under the title, with a note on how a box gets out of the mode it falls
 * under. Laid out as Koharu's preference rows are, with the note under the description, as the
 * colour detection switch above it is. */
export function SfxSection() {
  const { t } = useTranslation()
  const { mode, set } = useSfxMode()
  const hintId = useId()
  const noteId = useId()
  const title = t('booharu.sfx.title')
  const names = Object.fromEntries(
    sfxModes.map((choice) => [choice, t(`booharu.sfx.modes.${choice}`)]),
  )

  return (
    <div className='grid grid-cols-[minmax(150px,0.9fr)_minmax(220px,1.1fr)] items-center gap-6 py-3.5'>
      <div>
        <p className='text-[12px] font-medium'>{title}</p>
        {mode && (
          <p id={hintId} className='mt-0.5 text-[10px] leading-4 text-muted-foreground'>
            {t(`booharu.sfx.hints.${mode}`)}
          </p>
        )}
        <p id={noteId} className='mt-1 text-[10px] leading-4 text-muted-foreground'>
          {t('booharu.sfx.note')}
        </p>
      </div>
      <div className='min-w-0'>
        <Select
          value={mode ?? null}
          items={names}
          disabled={!mode}
          onValueChange={(next) => next && set(next as SfxMode)}
        >
          <SelectTrigger
            aria-label={title}
            // What the chosen mode does, and the note, read with the choice.
            aria-describedby={mode ? `${hintId} ${noteId}` : noteId}
            className='h-8 w-full text-[11px]'
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {sfxModes.map((choice) => (
              <SelectItem key={choice} value={choice} label={names[choice]}>
                <span className='grid gap-0.5 py-0.5 whitespace-normal'>
                  <span>{names[choice]}</span>
                  <span className='text-[10px] leading-4 text-muted-foreground'>
                    {t(`booharu.sfx.hints.${choice}`)}
                  </span>
                </span>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    </div>
  )
}
