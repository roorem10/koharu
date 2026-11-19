'use client'

import { useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { joinPages, readTime } from '@/lib/booharu/brief'
import { setChapterBrief, type FullChapter } from '@/lib/booharu/context'
import { useKoharuStore } from '@/lib/store'
import { modelName } from '@/lib/translation'
import { Badge } from '@koharu/ui/components/badge'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from '@koharu/ui/components/dialog'
import { Textarea } from '@koharu/ui/components/textarea'

/** A chapter's brief, as Read chapter writes it, with every field. */
type Brief = NonNullable<FullChapter['context']['brief']>

/** The Context tab's brief line, as Booharu 0.0.4 showed it: Chapter brief, when the chapter was
 * last read, in the user's language and time zone, and with which model, or that it hasn't been
 * read yet; and View, which opens the brief. A summary the user wrote without a read still shows as
 * not read. */
export function ChapterBriefLine({ brief }: { brief: Brief | null }) {
  const { t, i18n } = useTranslation()
  const models = useKoharuStore((state) => state.models)
  const [open, setOpen] = useState(false)
  const heading = useId()
  const status = brief?.read_at
    ? t('booharu.context.lastReadWith', {
        time: readTime(brief.read_at, i18n.language),
        model: brief.read_model
          ? modelName(models, brief.read_model)
          : t('booharu.context.unknownModel'),
      })
    : t('booharu.context.notRead')

  return (
    <div className='flex min-w-0 items-center gap-2'>
      <div className='min-w-0 flex-1'>
        <h3 id={heading} className='font-semibold'>
          {t('booharu.context.brief')}
        </h3>
        <p className='truncate text-[10px] text-muted-foreground' title={status}>
          {status}
        </p>
      </div>
      <Button
        type='button'
        size='xs'
        variant='outline'
        aria-describedby={heading}
        onClick={() => setOpen(true)}
      >
        {t('booharu.context.view')}
      </Button>
      {open && <BriefDialog brief={brief} onClose={() => setOpen(false)} />}
    </div>
  )
}

/** The brief, to read and edit: its summary, which Save brief saves as the user's, kept when the
 * chapter is read again, and the chapter's characters, each with the pages they're on. The summary
 * follows the chapter until the user types in it. A failed save shows its error, as every failed
 * command's does, and keeps the dialog. */
function BriefDialog({ brief, onClose }: { brief: Brief | null; onClose: () => void }) {
  const { t, i18n } = useTranslation()
  const charactersHeading = useId()
  const [draft, setDraft] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const saved = brief?.summary ?? ''
  const summary = draft ?? saved
  const changed = draft !== null && draft !== saved

  const save = () => {
    setBusy(true)
    setChapterBrief(summary)
      .then(() => onClose())
      .catch(() => undefined)
      .finally(() => setBusy(false))
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent showCloseButton={false} className='gap-3 sm:max-w-lg'>
        <DialogTitle className='text-[13px] font-semibold'>
          {t('booharu.context.brief')}
        </DialogTitle>
        <DialogDescription className='text-[11px]'>
          {t('booharu.context.briefDescription')}
        </DialogDescription>
        <label className='grid gap-1 text-[11px] font-medium'>
          {t('booharu.context.summary')}
          <Textarea
            autoFocus
            value={summary}
            disabled={busy}
            className='max-h-72 min-h-32 text-[12px] font-normal'
            onChange={(event) => setDraft(event.currentTarget.value)}
          />
        </label>
        {brief && brief.characters.length > 0 && (
          <section className='grid gap-1.5'>
            <h4 id={charactersHeading} className='text-[11px] font-medium'>
              {t('booharu.context.characters')}
            </h4>
            <ul aria-labelledby={charactersHeading} className='flex flex-wrap gap-1'>
              {brief.characters.map((character, index) => (
                <li key={`${index}:${character.name}:${character.original}`} className='max-w-full'>
                  <Badge
                    variant='outline'
                    className='h-auto max-w-full flex-wrap justify-start gap-x-1 py-0.5 text-[10px] font-normal break-words whitespace-normal'
                  >
                    <span className='font-medium'>{character.name || character.original}</span>
                    {character.name &&
                      character.original &&
                      character.original !== character.name && (
                        <span className='text-muted-foreground'>{character.original}</span>
                      )}
                    {character.pages.length > 0 && (
                      <span className='text-muted-foreground'>
                        ·{' '}
                        {t('booharu.context.characterPages', {
                          count: character.pages.length,
                          pages: joinPages(character.pages, i18n.language),
                        })}
                      </span>
                    )}
                  </Badge>
                </li>
              ))}
            </ul>
          </section>
        )}
        <DialogFooter>
          <Button type='button' variant='ghost' disabled={busy} onClick={onClose}>
            {t('booharu.context.close')}
          </Button>
          <Button type='button' disabled={busy || !changed} aria-busy={busy} onClick={save}>
            {t('booharu.context.saveBrief')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
