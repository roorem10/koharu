'use client'

import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { refreshContext, useChapterContext } from '@/components/context/data'
import { discussLine, historyChangedEvent } from '@/components/context/events'
import { call } from '@/lib/backend'
import { pageKey, projectKey, refresh, usePage, useProject } from '@/lib/queries'
import { commands, type EntityId, type LineExplanation } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

// Owned by WO-18: per-line chapter context (glossary terms, flags, Explain).
export function LineContextCard({ layer }: { layer: EntityId }) {
  const { t } = useTranslation()
  const chapter = useChapterContext().data
  const page = usePage().data
  const project = useProject().data
  const selected = page?.layers.find((item) => item.id === layer)
  const source = selected?.type === 'text' ? (selected.content.source?.text ?? '') : ''
  const translation = selected?.type === 'text' ? (selected.content.translation?.text ?? '') : ''
  const currentLayer = useRef(layer)
  currentLayer.current = layer
  const [explanation, setExplanation] = useState<LineExplanation | null>(null)
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    setExplanation(null)
  }, [layer, project?.revision])
  const line = chapter?.context.lines[layer]
  if (!chapter) return null
  return (
    <div className='grid gap-2 border-t pt-3 text-[11px]'>
      {line?.note && <p className='break-words whitespace-pre-wrap'>{line.note}</p>}
      {line?.flags.map((flag, index) => (
        <div className='grid gap-1 rounded-md border p-2' key={index}>
          <p>
            {flag.kind === 'glossary'
              ? t('chapterContext.glossaryFlag', { term: flag.term })
              : flag.message}
          </p>
          <Button
            size='sm'
            variant='ghost'
            onClick={() =>
              void call(commands.dismissLineFlag, layer, index)
                .then(refreshContext)
                .catch(() => undefined)
            }
          >
            {t('chapterContext.dismiss')}
          </Button>
        </div>
      ))}
      <Button
        size='sm'
        variant='outline'
        disabled={busy || !chapter.supports_tasks}
        title={!chapter.supports_tasks ? t('chapterContext.needsModel') : undefined}
        onClick={() => {
          setBusy(true)
          void call(commands.explainLine, layer)
            .then((value) => {
              if (currentLayer.current === layer) setExplanation(value)
              window.dispatchEvent(new Event(historyChangedEvent))
            })
            .catch(() => undefined)
            .finally(() => setBusy(false))
        }}
      >
        {busy ? t('chapterContext.explaining') : t('chapterContext.explain')}
      </Button>
      {explanation && (
        <div className='grid gap-2 rounded-md border bg-muted/30 p-2'>
          <p className='break-words whitespace-pre-wrap'>{explanation.explanation}</p>
          {explanation.alternatives.map((text) => (
            <div className='grid gap-1 border-t pt-2' key={text}>
              <p className='break-words'>{text}</p>
              <Button
                size='sm'
                variant='outline'
                onClick={() =>
                  void call(commands.setTranslation, layer, text)
                    .then(() => refresh(projectKey, pageKey))
                    .catch(() => undefined)
                }
              >
                {t('chapterContext.useThis')}
              </Button>
            </div>
          ))}
          <Button
            size='sm'
            variant='ghost'
            onClick={() =>
              discussLine(
                t('chapterContext.discussPrompt', {
                  layer,
                  source,
                  translation,
                  explanation: explanation.explanation,
                }),
              )
            }
          >
            {t('chapterContext.discuss')}
          </Button>
        </div>
      )}
    </div>
  )
}
