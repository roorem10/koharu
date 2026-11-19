'use client'

import { Check } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { ReadingModelPicker } from '@/components/booharu/context/ReadingModelPicker'
import { useReadChapter } from '@/lib/booharu/readChapter'
import { useKoharuStore } from '@/lib/store'
import { modelName } from '@/lib/translation'
import { Button } from '@koharu/ui/components/button'

/** Read chapter among the Run menu's stages, before Translation, laid out as they are: on, a Run
 * press that translates reads the whole chapter first, with the model its line names, and never
 * asks. It's the open project's while the app runs, as Koharu's own switches aren't saved either;
 * until it's switched, or a preset sets it, it's on when the chapter links a series, has no brief,
 * and a model can read it. While Translation is off it's off, and says why. Switched on with no
 * model that can read the pages, it opens the reading-model picker in its place: a pick saves the
 * Read chapter model and switches it on, and Cancel leaves it off. */
export function ReadChapterOption() {
  const { t } = useTranslation()
  const models = useKoharuStore((state) => state.models)
  const translates = useKoharuStore((state) => state.processingStages.includes('translate'))
  const { project, reader, on, set } = useReadChapter()
  const [picking, setPicking] = useState(false)

  if (picking) {
    return (
      <div className='rounded-lg border border-border/60 p-0.5'>
        <ReadingModelPicker
          onCancel={() => setPicking(false)}
          onPicked={() => {
            setPicking(false)
            set(true)
          }}
        />
      </div>
    )
  }

  const selected = translates && on
  const detail = !translates
    ? t('booharu.runs.read.needsTranslation')
    : reader === null
      ? t('booharu.runs.read.pickModel')
      : t('booharu.runs.read.hint', { model: modelName(models, reader) })

  return (
    <Button
      type='button'
      variant='ghost'
      aria-pressed={selected}
      disabled={!translates || project === null}
      className='h-auto min-h-9 justify-start gap-2 rounded-lg px-2 py-1 text-left font-normal hover:bg-primary/10'
      onClick={() => {
        if (selected) set(false)
        else if (reader === null) setPicking(true)
        else set(true)
      }}
    >
      <span className='min-w-0 flex-1'>
        <span className='block text-[11px]'>{t('booharu.runs.read.label')}</span>
        <span className='block text-[9px] text-muted-foreground'>{detail}</span>
      </span>
      {selected && <Check className='size-3.5 shrink-0 text-primary' />}
    </Button>
  )
}
