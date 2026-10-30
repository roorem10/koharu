'use client'

import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { commands } from '@koharu/bridge/protocol'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@koharu/ui/components/context-menu'

import { refreshContext, useChapterContext } from './data'
import { TermUsage } from './GlossaryDialog'
import { SeriesPicker } from './SeriesPicker'
import { newTerm, TermForm } from './TermForm'
import type { Series } from './types'

export function GlossaryTextMenu({
  children,
  field,
  source,
  translation,
}: {
  children: ReactNode
  field: 'source' | 'translation'
  source: string
  translation: string
}) {
  const { t } = useTranslation()
  const chapter = useChapterContext().data
  const [selection, setSelection] = useState('')
  const [open, setOpen] = useState(false)
  const [used, setUsed] = useState('')
  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger
          className='min-w-0'
          onContextMenuCapture={(event) => {
            const field = event.target
            if (field instanceof HTMLTextAreaElement)
              setSelection(field.value.slice(field.selectionStart, field.selectionEnd))
          }}
        >
          {children}
        </ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem disabled={!selection.trim()} onClick={() => setOpen(true)}>
            {t('chapterContext.addToGlossary')}
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
      {open && !chapter?.series && (
        <div className='grid gap-1 rounded border p-2 text-xs'>
          <p>{t('chapterContext.chooseSeriesHint')}</p>
          <SeriesPicker
            value={null}
            onChange={async (id) => {
              await call(commands.setProjectSeries, id)
              await refreshContext()
            }}
          />
        </div>
      )}
      {open && chapter?.series && (
        <TermForm
          initial={newTerm(
            field === 'source' ? selection : source,
            field === 'translation' ? selection : translation,
          )}
          onClose={() => setOpen(false)}
          onSave={async (entry) => {
            const series = (await call(commands.getSeries, chapter.series!.id)) as Series
            const existing = series.glossary.find(
              (term) => term.source.trim() === entry.source.trim(),
            )
            await call(commands.saveSeries, {
              ...series,
              glossary: [
                ...series.glossary.filter((term) => term.id !== existing?.id),
                { ...entry, id: existing?.id ?? entry.id },
              ],
            })
            setUsed(entry.source)
            await refreshContext()
          }}
        />
      )}
      {used && <TermUsage source={used} />}
    </>
  )
}
