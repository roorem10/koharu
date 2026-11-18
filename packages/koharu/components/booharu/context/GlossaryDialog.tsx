'use client'

import { Lock, Plus, Trash2, X } from 'lucide-react'
import { useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { CommitInput } from '@/components/booharu/context/CommitInput'
import { SeriesPicker } from '@/components/booharu/context/SeriesPicker'
import { TermForm } from '@/components/booharu/context/TermForm'
import { CommitTextarea } from '@/components/controls/CommitTextarea'
import {
  changeCharacter,
  changeSeries,
  deleteSeries,
  failure,
  kindName,
  matchesSearch,
  newCharacter,
  newTerm,
  putTerm,
  removeTerm,
  renameSeries,
  useSeries,
  useSeriesList,
  type FullCharacter,
  type FullEntry,
  type FullSeries,
} from '@/lib/booharu/context'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@koharu/ui/components/alert-dialog'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@koharu/ui/components/dialog'
import { Input } from '@koharu/ui/components/input'

/** The glossary editor, Booharu 0.0.4's glossary dialog: a series picked or made here, starting on
 * `initial`, such as the series the open chapter follows, with its name, its notes, its characters
 * and its terms, each change saved at once over what its file holds then, so a change another
 * writer made meanwhile, such as a term the agent added, stays. Opened from the Context tab and
 * from Settings > Pipeline's Glossary row; `onClose` closes it. */
export function GlossaryDialog({
  initial,
  onClose,
}: {
  initial: string | null
  onClose: () => void
}) {
  const { t } = useTranslation()
  const listed = useSeriesList().data
  const [selected, setSelected] = useState(initial)
  // A series deleted or gone since, as the agent can delete one, shows as none picked.
  const current =
    selected !== null && (listed === undefined || listed.some((series) => series.id === selected))
      ? selected
      : null

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent
        showCloseButton={false}
        className='flex max-h-[calc(100dvh-4rem)] flex-col gap-3 overflow-hidden p-4 sm:max-w-3xl'
      >
        <DialogTitle className='pr-8 text-[13px] font-semibold'>
          {t('booharu.context.glossary')}
        </DialogTitle>
        <DialogDescription className='sr-only'>
          {t('booharu.context.glossaryDescription')}
        </DialogDescription>
        <DialogClose
          render={
            <Button
              variant='ghost'
              size='icon-sm'
              className='absolute top-2 right-2'
              aria-label={t('booharu.context.close')}
            />
          }
        >
          <X />
        </DialogClose>
        <SeriesPicker value={current} onChange={async (id) => setSelected(id)} />
        {current === null ? (
          <p className='text-[11px] text-muted-foreground'>
            {t('booharu.context.chooseSeriesHint')}
          </p>
        ) : (
          <SeriesEditor key={current} id={current} onDeleted={() => setSelected(null)} />
        )}
      </DialogContent>
    </Dialog>
  )
}

/** The series `id` in the glossary editor, read again after every change to a series. */
function SeriesEditor({ id, onDeleted }: { id: string; onDeleted: () => void }) {
  const { t } = useTranslation()
  const series = useSeries(id)

  if (series.error) {
    return (
      <div role='alert' className='grid gap-1 text-[11px] leading-4'>
        <p className='font-medium'>{t('booharu.context.seriesFailed')}</p>
        <p className='break-words text-muted-foreground'>{failure(series.error)}</p>
      </div>
    )
  }
  if (!series.data) {
    return <p className='text-[11px] text-muted-foreground'>{t('common.loading')}</p>
  }
  return (
    <div className='-mx-4 grid min-h-0 gap-5 overflow-y-auto px-4 pb-1 text-[11px]'>
      <SeriesDetails series={series.data} onDeleted={onDeleted} />
      <Characters series={series.data} />
      <Terms series={series.data} />
    </div>
  )
}

/** A series' name, renamed as it's committed, Delete series, which asks first, and its notes. */
function SeriesDetails({ series, onDeleted }: { series: FullSeries; onDeleted: () => void }) {
  const { t } = useTranslation()
  const ids = useId()
  const [confirming, setConfirming] = useState(false)
  const [deleting, setDeleting] = useState(false)

  return (
    <section className='grid gap-3'>
      <div className='grid gap-1'>
        <label htmlFor={`${ids}-name`} className='font-medium'>
          {t('booharu.context.seriesName')}
        </label>
        <div className='flex min-w-0 items-center gap-2'>
          <CommitInput
            id={`${ids}-name`}
            required
            value={series.name}
            className='h-8 text-[12px]'
            onCommit={(name) => renameSeries(series.id, name)}
          />
          <Button
            type='button'
            size='sm'
            variant='destructive'
            className='shrink-0'
            onClick={() => setConfirming(true)}
          >
            <Trash2 /> {t('booharu.context.deleteSeries')}
          </Button>
        </div>
      </div>
      <div className='grid gap-1'>
        <label htmlFor={`${ids}-notes`} className='font-medium'>
          {t('booharu.context.seriesNotes')}
        </label>
        <p id={`${ids}-notes-hint`} className='text-[10px] text-muted-foreground'>
          {t('booharu.context.seriesNotesDescription')}
        </p>
        <CommitTextarea
          id={`${ids}-notes`}
          aria-describedby={`${ids}-notes-hint`}
          value={series.notes}
          className='min-h-20 text-[12px]'
          onCommit={(notes) =>
            void changeSeries(series.id, (held) => ({ ...held, notes })).catch(() => undefined)
          }
        />
      </div>
      <AlertDialog
        open={confirming}
        onOpenChange={(open) => {
          if (!open && !deleting) setConfirming(false)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('booharu.context.deleteSeries')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t('booharu.context.deleteSeriesConfirm')}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleting}>{t('common.cancel')}</AlertDialogCancel>
            <AlertDialogAction
              variant='destructive'
              disabled={deleting}
              aria-busy={deleting}
              onClick={() => {
                setDeleting(true)
                deleteSeries(series.id)
                  .then(() => {
                    setConfirming(false)
                    onDeleted()
                  })
                  .catch(() => undefined)
                  .finally(() => setDeleting(false))
              }}
            >
              {t('booharu.context.delete')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </section>
  )
}

/** A series' characters, each with their name, original name and voice notes, each change saved as
 * it's committed; Add character adds one with nothing written, its name field focused. */
function Characters({ series }: { series: FullSeries }) {
  const { t } = useTranslation()
  const heading = useId()
  // The character just added, whose name field takes the focus as it shows.
  const [added, setAdded] = useState<string | null>(null)
  const [adding, setAdding] = useState(false)

  const change = (character: FullCharacter, values: Partial<FullCharacter>) =>
    changeSeries(series.id, (held) => changeCharacter(held, character.id, values))

  return (
    <section className='grid gap-2' aria-labelledby={heading}>
      <h3 id={heading} className='font-semibold'>
        {t('booharu.context.characters')}
      </h3>
      {series.characters.map((character) => (
        <div
          key={character.id}
          role='group'
          aria-label={character.name || t('booharu.context.newCharacter')}
          className='grid gap-1.5 rounded-lg border border-border/70 p-2'
        >
          <div className='flex min-w-0 items-center gap-1.5'>
            <CommitInput
              autoFocus={character.id === added}
              aria-label={t('booharu.context.characterName')}
              placeholder={t('booharu.context.characterName')}
              value={character.name}
              className='h-7 text-[12px]'
              onCommit={(name) => change(character, { name })}
            />
            <CommitInput
              aria-label={t('booharu.context.originalName')}
              placeholder={t('booharu.context.originalName')}
              value={character.original}
              className='h-7 text-[12px]'
              onCommit={(original) => change(character, { original })}
            />
            <Button
              type='button'
              size='icon-sm'
              variant='ghost'
              aria-label={t('booharu.context.deleteCharacter')}
              onClick={() =>
                void changeSeries(series.id, (held) => ({
                  ...held,
                  characters: held.characters.filter((item) => item.id !== character.id),
                })).catch(() => undefined)
              }
            >
              <Trash2 />
            </Button>
          </div>
          <CommitTextarea
            aria-label={t('booharu.context.voiceNotes')}
            placeholder={t('booharu.context.voiceNotes')}
            value={character.notes}
            className='min-h-12 text-[12px]'
            onCommit={(notes) => void change(character, { notes }).catch(() => undefined)}
          />
        </div>
      ))}
      <Button
        type='button'
        size='sm'
        variant='outline'
        className='justify-self-start'
        disabled={adding}
        onClick={() => {
          const character = newCharacter()
          setAdding(true)
          changeSeries(series.id, (held) => ({
            ...held,
            characters: [...held.characters, character],
          }))
            .then(() => setAdded(character.id))
            .catch(() => undefined)
            .finally(() => setAdding(false))
        }}
      >
        <Plus /> {t('booharu.context.addCharacter')}
      </Button>
    </section>
  )
}

/** A series' terms, searchable, each to edit in the term form or delete, and Add term. */
function Terms({ series }: { series: FullSeries }) {
  const { t } = useTranslation()
  const heading = useId()
  const [search, setSearch] = useState('')
  const [editing, setEditing] = useState<FullEntry | null>(null)
  // The term a Delete is under way for, whose buttons wait for it.
  const [deleting, setDeleting] = useState<string | null>(null)
  const shown = series.glossary.filter((entry) => matchesSearch(entry, search))
  const cell = 'max-w-56 px-2 py-1.5 align-top break-words'

  return (
    <section className='grid gap-2' aria-labelledby={heading}>
      <h3 id={heading} className='font-semibold'>
        {t('booharu.context.terms')}
      </h3>
      <div className='flex min-w-0 items-center gap-2'>
        <Input
          type='search'
          value={search}
          aria-label={t('booharu.context.searchGlossary')}
          placeholder={t('booharu.context.searchGlossary')}
          className='h-8 text-[12px]'
          onChange={(event) => setSearch(event.currentTarget.value)}
        />
        <Button type='button' size='sm' className='shrink-0' onClick={() => setEditing(newTerm())}>
          <Plus /> {t('booharu.context.addTerm')}
        </Button>
      </div>
      {series.glossary.length === 0 ? (
        <p className='text-muted-foreground'>{t('booharu.context.noTerms')}</p>
      ) : shown.length === 0 ? (
        <p className='text-muted-foreground'>{t('booharu.context.noMatch')}</p>
      ) : (
        <div className='max-h-80 overflow-auto rounded-lg border border-border/70'>
          <table className='w-full text-left text-[11px]'>
            <thead className='sticky top-0 bg-muted text-[10px] text-muted-foreground'>
              <tr>
                <th className='px-2 py-1.5 font-medium'>{t('booharu.context.source')}</th>
                <th className='px-2 py-1.5 font-medium'>{t('booharu.context.target')}</th>
                <th className='px-2 py-1.5 font-medium'>{t('booharu.context.kind')}</th>
                <th className='px-2 py-1.5 font-medium'>
                  <span className='sr-only'>{t('booharu.context.actions')}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {shown.map((entry, index) => (
                <tr key={`${entry.id}:${index}`} className='border-t border-border/60'>
                  <td className={cell}>
                    {entry.source}
                    {entry.locked && (
                      <Lock
                        role='img'
                        aria-label={t('booharu.context.locked')}
                        className='ml-1 inline size-3 align-[-1px] text-muted-foreground'
                      />
                    )}
                  </td>
                  <td className={cell}>
                    {entry.target}
                    {entry.note && (
                      <span className='block text-[10px] text-muted-foreground'>{entry.note}</span>
                    )}
                  </td>
                  <td className='px-2 py-1.5 align-top whitespace-nowrap'>
                    {t(kindName(entry.kind))}
                  </td>
                  <td className='px-2 py-1 align-top'>
                    <div className='flex justify-end gap-1'>
                      <Button
                        type='button'
                        size='xs'
                        variant='ghost'
                        disabled={deleting === entry.id}
                        onClick={() => setEditing(entry)}
                      >
                        {t('booharu.context.edit')}
                      </Button>
                      <Button
                        type='button'
                        size='xs'
                        variant='ghost'
                        disabled={deleting === entry.id}
                        onClick={() => {
                          setDeleting(entry.id)
                          changeSeries(series.id, (held) => removeTerm(held, entry.id))
                            .catch(() => undefined)
                            .finally(() => setDeleting(null))
                        }}
                      >
                        {t('booharu.context.delete')}
                      </Button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {editing && (
        <TermForm
          initial={editing}
          glossary={series.glossary}
          onClose={() => setEditing(null)}
          onSave={(entry) => changeSeries(series.id, (held) => putTerm(held, entry))}
        />
      )}
    </section>
  )
}
