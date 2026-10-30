'use client'

import { Lock } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { usePages, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/bridge/protocol'
import { Badge } from '@koharu/ui/components/badge'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import { Input } from '@koharu/ui/components/input'
import { ScrollArea } from '@koharu/ui/components/scroll-area'
import { Textarea } from '@koharu/ui/components/textarea'

import { jumpToFlag, refreshContext, useChapterContext } from './data'
import { GlossaryDialog } from './GlossaryDialog'
import { SeriesPicker } from './SeriesPicker'
import { TermForm } from './TermForm'
import type { GlossaryEntry } from './types'

export function ContextPanel() {
  const { t } = useTranslation()
  const project = useProject().data
  const chapter = useChapterContext()
  const pages = usePages().data ?? []
  const [summary, setSummary] = useState('')
  const [dirty, setDirty] = useState(false)
  const [textOnly, setTextOnly] = useState(false)
  useEffect(() => {
    setDirty(false)
    setSummary('')
  }, [project?.name])
  const [search, setSearch] = useState('')
  const [glossaryOpen, setGlossaryOpen] = useState(false)
  const [editing, setEditing] = useState<GlossaryEntry | null>(null)
  const jobs = useKoharuStore((state) => state.jobs)
  const running = Object.values(jobs).some((job) => job.state === 'running')
  useEffect(() => {
    if (!dirty) setSummary(chapter.data?.context.brief?.summary ?? '')
  }, [chapter.data?.context.brief?.summary, dirty])
  useEffect(() => {
    void call(commands.getContextSettings)
      .then((settings) => setTextOnly(!settings.send_images))
      .catch(() => undefined)
  }, [])
  const data = chapter.data
  if (!data)
    return (
      <p className='p-3 text-xs text-muted-foreground'>
        {chapter.isError ? t('chapterContext.loadFailed') : t('chapterContext.loading')}
      </p>
    )
  const terms = [...(data.series?.glossary ?? []), ...data.context.suggestions]
  const flags = Object.entries(data.context.lines).flatMap(([layer, line]) =>
    line.flags.map((flag, index) => ({ layer, flag, index })),
  )
  const readAt = data.context.brief?.read_at
  return (
    <ScrollArea className='min-h-0 flex-1'>
      <div className='grid min-w-0 gap-4 p-3 text-xs'>
        <SeriesPicker
          value={data.series?.id ?? null}
          onChange={async (id) => {
            await call(commands.setProjectSeries, id)
            await refreshContext()
          }}
        />
        {!data.series && (
          <p className='text-muted-foreground'>{t('chapterContext.chooseSeriesHint')}</p>
        )}
        <section className='grid gap-2'>
          <Button
            size='sm'
            disabled={running || !data.series || !data.supports_tasks}
            title={!data.supports_tasks ? t('chapterContext.needsModel') : undefined}
            onClick={() =>
              void call(commands.readChapter, { scope: 'project' }, textOnly)
                .then(refreshContext)
                .catch(() => undefined)
            }
          >
            {t('chapterContext.readChapter')}
          </Button>
          {!data.supports_tasks && (
            <p className='text-muted-foreground'>{t('chapterContext.needsModel')}</p>
          )}
          <span className='text-muted-foreground'>
            {readAt
              ? t('chapterContext.lastRead', {
                  time: new Date(Number(readAt) * 1000).toLocaleString(),
                })
              : t('chapterContext.notRead')}
          </span>
          <label className='flex items-center gap-2'>
            <Checkbox checked={textOnly} onCheckedChange={(checked) => setTextOnly(checked)} />
            {t('chapterContext.textOnly')}
          </label>
        </section>
        <section className='grid gap-2'>
          <h3 className='font-semibold'>{t('chapterContext.brief')}</h3>
          <Textarea
            aria-label={t('chapterContext.brief')}
            value={summary}
            className='min-h-32 text-xs'
            onChange={(event) => {
              setSummary(event.target.value)
              setDirty(true)
            }}
          />
          {dirty && (
            <Button
              size='sm'
              onClick={() =>
                void call(commands.saveChapterSummary, summary)
                  .then(async () => {
                    setDirty(false)
                    await refreshContext()
                  })
                  .catch(() => undefined)
              }
            >
              {t('chapterContext.saveBrief')}
            </Button>
          )}
          <div className='flex flex-wrap gap-1'>
            {data.context.brief?.characters.map((character) => (
              <Badge
                className='max-w-full break-words whitespace-normal'
                variant='outline'
                key={`${character.name}:${character.original}`}
              >
                {character.name}
              </Badge>
            ))}
          </div>
        </section>
        <section className='grid min-w-0 gap-2'>
          <h3 className='font-semibold'>{t('chapterContext.glossary')}</h3>
          <Input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={t('chapterContext.searchGlossary')}
            aria-label={t('chapterContext.searchGlossary')}
          />
          <div className='grid max-h-72 gap-2 overflow-y-auto'>
            {terms
              .filter((entry) =>
                `${entry.source} ${entry.target}`.toLowerCase().includes(search.toLowerCase()),
              )
              .map((entry) => {
                const suggested = data.context.suggestions.some((term) => term.id === entry.id)
                return (
                  <div className='min-w-0 rounded-md border p-2' key={entry.id}>
                    <p className='break-words'>
                      {entry.source} → {entry.target}{' '}
                      {entry.locked && (
                        <Lock className='inline size-3' aria-label={t('chapterContext.locked')} />
                      )}
                    </p>
                    <span className='text-[10px] text-muted-foreground'>
                      {t(`chapterContext.kinds.${entry.kind}`)}
                    </span>
                    {suggested && (
                      <>
                        <Badge variant='outline' className='ml-1 text-[9px]'>
                          {t('chapterContext.suggested')}
                        </Badge>
                        <div className='mt-1 flex flex-wrap gap-1'>
                          <Button
                            size='sm'
                            variant='ghost'
                            onClick={() =>
                              void call(commands.acceptSuggestion, entry)
                                .then(refreshContext)
                                .catch(() => undefined)
                            }
                          >
                            {t('chapterContext.accept')}
                          </Button>
                          <Button size='sm' variant='ghost' onClick={() => setEditing(entry)}>
                            {t('chapterContext.edit')}
                          </Button>
                          <Button
                            size='sm'
                            variant='ghost'
                            onClick={() =>
                              void call(commands.dismissSuggestion, entry.id)
                                .then(refreshContext)
                                .catch(() => undefined)
                            }
                          >
                            {t('chapterContext.dismiss')}
                          </Button>
                        </div>
                      </>
                    )}
                  </div>
                )
              })}
            {terms.length === 0 && (
              <p className='text-muted-foreground'>{t('chapterContext.noTerms')}</p>
            )}
          </div>
          <Button size='sm' variant='outline' onClick={() => setGlossaryOpen(true)}>
            {t('chapterContext.openGlossary')}
          </Button>
        </section>
        <section className='grid gap-2'>
          <div className='flex flex-wrap items-center justify-between gap-1'>
            <h3 className='font-semibold'>{t('chapterContext.flags')}</h3>
            <Button
              size='sm'
              variant='ghost'
              onClick={() =>
                void call(commands.checkGlossary)
                  .then(refreshContext)
                  .catch(() => undefined)
              }
            >
              {t('chapterContext.checkGlossary')}
            </Button>
          </div>
          {flags.length === 0 && (
            <p className='text-muted-foreground'>{t('chapterContext.noFlags')}</p>
          )}
          {flags.map(({ layer, flag, index }) => (
            <Button
              key={`${layer}:${index}`}
              className='h-auto justify-start text-left text-xs whitespace-normal'
              size='sm'
              variant='outline'
              onClick={() => void jumpToFlag(layer, flag, pages).catch(() => undefined)}
            >
              {t('chapterContext.flagPage', { page: flag.page })} ·{' '}
              {flag.kind === 'glossary'
                ? t('chapterContext.glossaryFlag', { term: flag.term })
                : flag.message}
            </Button>
          ))}
        </section>
        {glossaryOpen && (
          <GlossaryDialog open onClose={() => setGlossaryOpen(false)} seriesId={data.series?.id} />
        )}
        {editing && (
          <TermForm
            initial={editing}
            onClose={() => setEditing(null)}
            onSave={async (entry) => {
              await call(commands.acceptSuggestion, entry)
              await refreshContext()
            }}
          />
        )}
      </div>
    </ScrollArea>
  )
}
