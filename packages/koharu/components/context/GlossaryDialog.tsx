'use client'

import { useQuery } from '@tanstack/react-query'
import { Lock } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { useProject } from '@/lib/queries'
import { commands } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Dialog, DialogContent, DialogTitle } from '@koharu/ui/components/dialog'
import { Input } from '@koharu/ui/components/input'
import { Textarea } from '@koharu/ui/components/textarea'

import { refreshContext, seriesKey, useSeries } from './data'
import { InstructionImport } from './InstructionImport'
import { SeriesPicker } from './SeriesPicker'
import { newTerm, TermForm } from './TermForm'
import type { GlossaryEntry, Series } from './types'

export function TermUsage({ source }: { source: string }) {
  const { t } = useTranslation()
  const project = useProject().data
  const usage = useQuery({
    queryKey: ['term-usage', project?.name, project?.revision, source],
    queryFn: () => call(commands.glossaryUsage, source),
    enabled: Boolean(project) && Boolean(source),
  }).data
  if (!usage) return null
  return (
    <div className='grid gap-1 rounded-md bg-muted p-2 text-xs'>
      <span>{t('chapterContext.usedOnLines', { count: usage.layers.length })}</span>
      {usage.skipped > 0 && (
        <span className='text-muted-foreground'>
          {t('chapterContext.handEditsSkipped', { count: usage.skipped })}
        </span>
      )}
      <Button
        size='sm'
        variant='outline'
        disabled={usage.layers.length === usage.skipped}
        onClick={() =>
          void call(
            commands.process,
            { scope: 'entities', value: usage.layers },
            { operation: 'only', stage: 'translation' },
          ).catch(() => undefined)
        }
      >
        {t('chapterContext.retranslate')}
      </Button>
    </div>
  )
}

export function GlossaryDialog({
  open,
  onClose,
  seriesId,
}: {
  open: boolean
  onClose: () => void
  seriesId?: string | null
}) {
  const { t } = useTranslation()
  const allSeries = useSeries().data ?? []
  const [selected, setSelected] = useState<string | null>(seriesId ?? null)
  const [search, setSearch] = useState('')
  const [editing, setEditing] = useState<GlossaryEntry | null>(null)
  const [importing, setImporting] = useState<Series | null>(null)
  const [usedSource, setUsedSource] = useState('')
  const [draft, setDraft] = useState<Series | null>(null)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const series = useQuery({
    queryKey: [...seriesKey, selected],
    queryFn: async () => (await call(commands.getSeries, selected!)) as Series,
    enabled: open && Boolean(selected),
  }).data
  useEffect(() => {
    if (open) setSelected(seriesId ?? null)
  }, [open, seriesId])
  useEffect(() => {
    setDraft(series ?? null)
  }, [series])
  async function save(value: Series) {
    setBusy(true)
    try {
      setDraft((await call(commands.saveSeries, value)) as Series)
      await refreshContext()
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog open={open} onOpenChange={(next) => !next && !busy && onClose()}>
      <DialogContent className='max-h-[85vh] overflow-y-auto sm:max-w-4xl' showCloseButton={false}>
        <div className='flex items-center justify-between gap-2'>
          <DialogTitle>{t('chapterContext.glossary')}</DialogTitle>
          <Button size='sm' variant='ghost' onClick={onClose}>
            {t('chapterContext.close')}
          </Button>
        </div>
        <SeriesPicker
          value={selected}
          onChange={async (id) => {
            setSelected(id)
          }}
        />
        <div className='flex flex-wrap gap-2'>
          <Button
            size='sm'
            variant='outline'
            onClick={() =>
              void call(commands.previewSeriesImport, t('chapterContext.importSeries'))
                .then((item) => {
                  if (item) setImporting(item as Series)
                })
                .catch(() => undefined)
            }
          >
            {t('chapterContext.importSeries')}
          </Button>
          {selected && (
            <>
              <Button
                size='sm'
                variant='outline'
                onClick={() =>
                  void call(
                    commands.exportSeries,
                    selected,
                    t('chapterContext.exportSeries'),
                  ).catch(() => undefined)
                }
              >
                {t('chapterContext.exportSeries')}
              </Button>
              <InstructionImport series={selected} />
            </>
          )}
        </div>
        {draft && (
          <>
            <div className='flex gap-2'>
              <Input
                aria-label={t('chapterContext.seriesName')}
                value={draft.name}
                onChange={(event) => setDraft({ ...draft, name: event.target.value })}
              />
              <Button
                size='sm'
                variant='outline'
                disabled={busy}
                onClick={() => setDeleteOpen(true)}
              >
                {t('chapterContext.deleteSeries')}
              </Button>
            </div>
            <label className='grid gap-1 text-xs'>
              {t('chapterContext.seriesNotes')}
              <Textarea
                value={draft.notes}
                onChange={(event) => setDraft({ ...draft, notes: event.target.value })}
              />
            </label>
            <details>
              <summary className='cursor-pointer text-xs'>{t('chapterContext.characters')}</summary>
              <div className='mt-2 grid gap-2'>
                {draft.characters.map((character, index) => (
                  <div className='grid gap-1 rounded-md border p-2' key={character.id}>
                    <Input
                      aria-label={t('chapterContext.characterName')}
                      value={character.name}
                      onChange={(event) =>
                        setDraft({
                          ...draft,
                          characters: draft.characters.map((item, i) =>
                            i === index ? { ...item, name: event.target.value } : item,
                          ),
                        })
                      }
                    />
                    <Input
                      aria-label={t('chapterContext.originalName')}
                      value={character.original}
                      onChange={(event) =>
                        setDraft({
                          ...draft,
                          characters: draft.characters.map((item, i) =>
                            i === index ? { ...item, original: event.target.value } : item,
                          ),
                        })
                      }
                    />
                    <Textarea
                      aria-label={t('chapterContext.voiceNotes')}
                      value={character.notes}
                      onChange={(event) =>
                        setDraft({
                          ...draft,
                          characters: draft.characters.map((item, i) =>
                            i === index ? { ...item, notes: event.target.value } : item,
                          ),
                        })
                      }
                    />
                    <Button
                      size='sm'
                      variant='ghost'
                      onClick={() =>
                        setDraft({
                          ...draft,
                          characters: draft.characters.filter((_, i) => i !== index),
                        })
                      }
                    >
                      {t('chapterContext.delete')}
                    </Button>
                  </div>
                ))}
                <Button
                  size='sm'
                  variant='outline'
                  onClick={() =>
                    setDraft({
                      ...draft,
                      characters: [
                        ...draft.characters,
                        { id: crypto.randomUUID(), name: '', original: '', notes: '' },
                      ],
                    })
                  }
                >
                  {t('chapterContext.addCharacter')}
                </Button>
              </div>
            </details>
            <Button
              size='sm'
              disabled={busy || !draft.name.trim()}
              onClick={() => void save(draft).catch(() => undefined)}
            >
              {t('chapterContext.saveSeries')}
            </Button>
            <div className='flex gap-2'>
              <Input
                value={search}
                aria-label={t('chapterContext.searchGlossary')}
                placeholder={t('chapterContext.searchGlossary')}
                onChange={(event) => setSearch(event.target.value)}
              />
              <Button size='sm' onClick={() => setEditing(newTerm())}>
                {t('chapterContext.addTerm')}
              </Button>
            </div>
            <div className='max-h-72 overflow-auto rounded-md border'>
              <table className='w-full min-w-[600px] text-left text-xs'>
                <thead className='sticky top-0 bg-muted'>
                  <tr>
                    <th className='p-2'>{t('chapterContext.source')}</th>
                    <th>{t('chapterContext.target')}</th>
                    <th>{t('chapterContext.kind')}</th>
                    <th>{t('chapterContext.actions')}</th>
                  </tr>
                </thead>
                <tbody>
                  {draft.glossary
                    .filter((entry) =>
                      `${entry.source} ${entry.target} ${entry.note}`
                        .toLowerCase()
                        .includes(search.toLowerCase()),
                    )
                    .map((entry) => (
                      <tr key={entry.id} className='border-t align-top'>
                        <td className='max-w-48 p-2 break-words'>
                          {entry.source}{' '}
                          {entry.locked && (
                            <Lock
                              className='inline size-3'
                              aria-label={t('chapterContext.locked')}
                            />
                          )}
                        </td>
                        <td className='max-w-48 p-2 break-words'>{entry.target}</td>
                        <td className='p-2'>{t(`chapterContext.kinds.${entry.kind}`)}</td>
                        <td className='p-2'>
                          <div className='flex gap-1'>
                            <Button size='sm' variant='ghost' onClick={() => setEditing(entry)}>
                              {t('chapterContext.edit')}
                            </Button>
                            <Button
                              size='sm'
                              variant='ghost'
                              disabled={busy}
                              onClick={() =>
                                void save({
                                  ...draft,
                                  glossary: draft.glossary.filter((term) => term.id !== entry.id),
                                }).catch(() => undefined)
                              }
                            >
                              {t('chapterContext.delete')}
                            </Button>
                          </div>
                        </td>
                      </tr>
                    ))}
                </tbody>
              </table>
            </div>
            {usedSource && <TermUsage source={usedSource} />}
          </>
        )}
        {editing && draft && (
          <TermForm
            initial={editing}
            onClose={() => setEditing(null)}
            onSave={async (entry) => {
              await save({
                ...draft,
                glossary: [
                  ...draft.glossary.filter(
                    (term) => term.id !== entry.id && term.source.trim() !== entry.source.trim(),
                  ),
                  entry,
                ],
              })
              setUsedSource(entry.source)
            }}
          />
        )}
        <Dialog open={Boolean(importing)} onOpenChange={(next) => !next && setImporting(null)}>
          <DialogContent showCloseButton={false}>
            <DialogTitle>{t('chapterContext.importSeries')}</DialogTitle>
            <p className='text-sm break-words'>{importing?.name}</p>
            <p className='text-xs'>{t('chapterContext.importChoice')}</p>
            <div className='flex flex-wrap gap-2'>
              {importing && allSeries.some((item) => item.id === importing.id) && (
                <Button onClick={() => void doImport(true)}>{t('chapterContext.replace')}</Button>
              )}
              <Button onClick={() => void doImport(false)}>{t('chapterContext.keepBoth')}</Button>
              <Button variant='ghost' onClick={() => setImporting(null)}>
                {t('chapterContext.cancel')}
              </Button>
            </div>
          </DialogContent>
        </Dialog>
        <Dialog open={deleteOpen} onOpenChange={setDeleteOpen}>
          <DialogContent showCloseButton={false}>
            <DialogTitle>{t('chapterContext.deleteSeries')}</DialogTitle>
            <p>{t('chapterContext.deleteSeriesConfirm')}</p>
            <div className='flex gap-2'>
              <Button
                variant='destructive'
                onClick={() => {
                  if (!selected) return
                  void call(commands.deleteSeries, selected)
                    .then(async () => {
                      setSelected(null)
                      setDraft(null)
                      setDeleteOpen(false)
                      await refreshContext()
                    })
                    .catch(() => undefined)
                }}
              >
                {t('chapterContext.delete')}
              </Button>
              <Button variant='ghost' onClick={() => setDeleteOpen(false)}>
                {t('chapterContext.cancel')}
              </Button>
            </div>
          </DialogContent>
        </Dialog>
      </DialogContent>
    </Dialog>
  )

  async function doImport(replace: boolean) {
    if (!importing) return
    try {
      const item = (await call(commands.importSeries, importing, replace)) as Series
      await refreshContext()
      setSelected(item.id)
      setImporting(null)
    } catch {
      /* call reports native errors */
    }
  }
}
