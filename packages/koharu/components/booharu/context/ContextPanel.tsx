'use client'

import { BookOpen, Lock } from 'lucide-react'
import { useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { GlossaryDialog } from '@/components/booharu/context/GlossaryDialog'
import { SeriesPicker } from '@/components/booharu/context/SeriesPicker'
import { TermForm } from '@/components/booharu/context/TermForm'
import { TermUsage } from '@/components/booharu/context/TermUsage'
import {
  acceptSuggestion,
  dismissSuggestion,
  failure,
  kindName,
  linkSeries,
  matchesSearch,
  replacedTerm,
  useChapterContext,
  type FullEntry,
} from '@/lib/booharu/context'
import { Badge } from '@koharu/ui/components/badge'
import { Button } from '@koharu/ui/components/button'
import { Input } from '@koharu/ui/components/input'
import { ScrollArea } from '@koharu/ui/components/scroll-area'

/** The Context tab's name in the right pane, with its icon, as Booharu 0.0.4 showed it. */
export function ContextTabLabel() {
  const { t } = useTranslation()
  return (
    <>
      <BookOpen /> {t('booharu.context.title')}
    </>
  )
}

/** A term the tab lists: the linked series' own, or one suggested for the chapter. */
interface Listed {
  entry: FullEntry
  suggested: boolean
}

/** The Context tab, Booharu 0.0.4's layout before Read chapter: the series the open chapter
 * follows, picked or made here; a line asking for one while it links none, or saying the one it
 * links is gone, or can't be read, with the error naming its file and Unlink series, so a broken
 * series file never leaves the chapter stuck on it; and the glossary its translations follow,
 * searchable, the series' terms then the terms suggested for the chapter, each suggestion to
 * accept into the series, to edit and accept, or to dismiss, a term accepted showing where it's
 * used, with Re-translate those lines; then Open full glossary, the glossary editor. Accepting a
 * suggestion whose source the series has opens the term form, which says what saving replaces,
 * rather than replacing that term at a click. A side file that doesn't read shows its error, which
 * names it. Everything is read again after every change to the chapter, its series or its pages. */
export function ContextPanel() {
  const { t } = useTranslation()
  const heading = useId()
  const chapter = useChapterContext()
  const [search, setSearch] = useState('')
  const [editing, setEditing] = useState<FullEntry | null>(null)
  const [glossaryOpen, setGlossaryOpen] = useState(false)
  // The suggestion an Accept or Dismiss is under way for, whose buttons wait for it.
  const [pending, setPending] = useState<string | null>(null)
  // The term accepted last, whose use shows, and how many were, so each shows anew.
  const [accepted, setAccepted] = useState<{ entry: FullEntry; count: number } | null>(null)
  const showUse = (entry: FullEntry) =>
    setAccepted((last) => ({ entry, count: (last?.count ?? 0) + 1 }))

  if (chapter.error) {
    return (
      <div role='alert' className='grid gap-1 p-3 text-[11px] leading-4'>
        <p className='font-medium'>{t('booharu.context.loadFailed')}</p>
        <p className='break-words text-muted-foreground'>{failure(chapter.error)}</p>
      </div>
    )
  }
  const data = chapter.data
  if (!data) {
    return <p className='p-3 text-[11px] text-muted-foreground'>{t('booharu.context.loading')}</p>
  }

  const linked = data.series
  const terms: Listed[] = [
    ...(linked?.glossary ?? []).map((entry) => ({ entry, suggested: false })),
    ...data.context.suggestions.map((entry) => ({ entry, suggested: true })),
  ]
  const shown = terms.filter(({ entry }) => matchesSearch(entry, search))

  /** Runs `command` on the suggestion `id`, its buttons waiting for it. */
  const settle = (id: string, command: Promise<unknown>) => {
    setPending(id)
    void command.catch(() => undefined).finally(() => setPending(null))
  }

  return (
    <ScrollArea className='min-h-0 flex-1'>
      <div className='grid min-w-0 gap-4 p-3 text-[11px]'>
        <section className='grid min-w-0 gap-1.5'>
          <SeriesPicker value={linked?.id ?? null} onChange={(id) => linkSeries(id)} />
          {data.unreadable ? (
            <div role='alert' className='grid min-w-0 gap-1 leading-4'>
              <p className='font-medium'>{t('booharu.context.unreadableSeries')}</p>
              <p className='break-words text-muted-foreground'>{data.unreadable}</p>
              <Button
                type='button'
                size='xs'
                variant='outline'
                className='justify-self-start'
                onClick={() => void linkSeries(null).catch(() => undefined)}
              >
                {t('booharu.context.unlinkSeries')}
              </Button>
            </div>
          ) : data.missing ? (
            <p className='text-muted-foreground'>{t('booharu.context.missingSeries')}</p>
          ) : (
            !linked && (
              <p className='text-muted-foreground'>{t('booharu.context.chooseSeriesHint')}</p>
            )
          )}
        </section>
        <section className='grid min-w-0 gap-2' aria-labelledby={heading}>
          <h3 id={heading} className='font-semibold'>
            {t('booharu.context.glossary')}
          </h3>
          <Input
            type='search'
            value={search}
            aria-label={t('booharu.context.searchGlossary')}
            placeholder={t('booharu.context.searchGlossary')}
            className='h-7 text-[11px]'
            onChange={(event) => setSearch(event.currentTarget.value)}
          />
          {accepted && (
            <TermUsage
              key={accepted.count}
              entry={accepted.entry}
              onClose={() => setAccepted(null)}
            />
          )}
          {terms.length === 0 ? (
            <p className='text-muted-foreground'>{t('booharu.context.noTerms')}</p>
          ) : shown.length === 0 ? (
            <p className='text-muted-foreground'>{t('booharu.context.noMatch')}</p>
          ) : (
            <ul aria-label={t('booharu.context.glossary')} className='grid min-w-0 gap-1.5'>
              {shown.map(({ entry, suggested }, index) => (
                <li
                  key={`${suggested ? 'suggested' : 'series'}:${entry.id}:${index}`}
                  className='grid min-w-0 gap-1 rounded-md border border-border/70 px-2 py-1.5'
                >
                  <p className='min-w-0 text-[12px] break-words'>
                    {entry.source} → {entry.target}
                    {entry.locked && (
                      <Lock
                        role='img'
                        aria-label={t('booharu.context.locked')}
                        className='ml-1 inline size-3 align-[-1px] text-muted-foreground'
                      />
                    )}
                  </p>
                  <div className='flex flex-wrap items-center gap-1 text-[10px] text-muted-foreground'>
                    <span>{t(kindName(entry.kind))}</span>
                    {suggested && (
                      <Badge variant='outline' className='h-4 px-1.5 text-[9px]'>
                        {t('booharu.context.suggested')}
                      </Badge>
                    )}
                  </div>
                  {entry.note && (
                    <p className='text-[10px] break-words text-muted-foreground'>{entry.note}</p>
                  )}
                  {suggested && (
                    <div className='flex flex-wrap gap-1'>
                      <Button
                        type='button'
                        size='xs'
                        variant='outline'
                        disabled={!linked || pending === entry.id}
                        onClick={() => {
                          const unlocked = { ...entry, locked: false }
                          // A term of its source the series has goes only once the form,
                          // which says so, is saved.
                          if (linked && replacedTerm(linked.glossary, unlocked)) {
                            setEditing(unlocked)
                            return
                          }
                          settle(
                            entry.id,
                            acceptSuggestion(unlocked).then(() => showUse(unlocked)),
                          )
                        }}
                      >
                        {t('booharu.context.accept')}
                      </Button>
                      <Button
                        type='button'
                        size='xs'
                        variant='ghost'
                        disabled={!linked || pending === entry.id}
                        onClick={() => setEditing(entry)}
                      >
                        {t('booharu.context.edit')}
                      </Button>
                      <Button
                        type='button'
                        size='xs'
                        variant='ghost'
                        disabled={pending === entry.id}
                        onClick={() => settle(entry.id, dismissSuggestion(entry.id))}
                      >
                        {t('booharu.context.dismiss')}
                      </Button>
                    </div>
                  )}
                </li>
              ))}
            </ul>
          )}
          <Button
            type='button'
            size='sm'
            variant='outline'
            className='text-[11px]'
            onClick={() => setGlossaryOpen(true)}
          >
            {t('booharu.context.openGlossary')}
          </Button>
        </section>
      </div>
      {editing && (
        <TermForm
          initial={editing}
          glossary={linked?.glossary}
          onClose={() => setEditing(null)}
          onSave={(entry) =>
            // Over a locked term only when the form said it replaces one.
            acceptSuggestion(entry, replacedTerm(linked?.glossary ?? [], entry) !== null).then(() =>
              showUse(entry),
            )
          }
        />
      )}
      {glossaryOpen && (
        <GlossaryDialog initial={linked?.id ?? null} onClose={() => setGlossaryOpen(false)} />
      )}
    </ScrollArea>
  )
}
