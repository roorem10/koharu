'use client'

import { useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { TermUsage } from '@/components/booharu/context/TermUsage'
import { kindName, replacedTerm, termKinds, type FullEntry } from '@/lib/booharu/context'
import type { TermKind } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from '@koharu/ui/components/dialog'
import { Input } from '@koharu/ui/components/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'
import { Textarea } from '@koharu/ui/components/textarea'

/** The term form, Booharu 0.0.4's glossary entry: a term's source, translation, kind and note,
 * and whether it's locked, starting from `initial`. Save, enabled once the source and translation
 * aren't blank, calls `onSave` with the term as written, and the form closes once it's saved; a
 * failed save shows its error, as every failed command's does, and keeps the form open. When
 * `glossary`, the series' terms, has another term with the source written, the form says saving
 * replaces it. With `showUsage`, a save leaves the form open on where the term is used, with
 * Re-translate those lines, until Done. */
export function TermForm({
  initial,
  glossary = [],
  showUsage = false,
  onSave,
  onClose,
}: {
  initial: FullEntry
  glossary?: readonly FullEntry[]
  showUsage?: boolean
  onSave: (entry: FullEntry) => Promise<unknown>
  onClose: () => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const [entry, setEntry] = useState(initial)
  const [busy, setBusy] = useState(false)
  // The term as saved, while the form shows where it's used.
  const [saved, setSaved] = useState<FullEntry | null>(null)
  const replaced = replacedTerm(glossary, entry)
  const complete = entry.source.trim() !== '' && entry.target.trim() !== ''
  const field = 'grid gap-1 text-[11px] font-medium'

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent showCloseButton={false} className='gap-3 sm:max-w-md'>
        <DialogTitle className='text-[13px] font-semibold'>
          {saved ? t('booharu.context.termSaved') : t('booharu.context.editTerm')}
        </DialogTitle>
        <DialogDescription className='sr-only'>
          {t('booharu.context.editTermDescription')}
        </DialogDescription>
        {saved ? (
          <>
            <TermUsage entry={saved} />
            <DialogFooter>
              <Button type='button' autoFocus onClick={onClose}>
                {t('booharu.context.done')}
              </Button>
            </DialogFooter>
          </>
        ) : (
          <form
            className='grid gap-3'
            onSubmit={(event) => {
              event.preventDefault()
              if (!complete || busy) return
              setBusy(true)
              onSave(entry)
                .then(() =>
                  showUsage
                    ? setSaved({
                        ...entry,
                        source: entry.source.trim(),
                        target: entry.target.trim(),
                      })
                    : onClose(),
                )
                .catch(() => undefined)
                .finally(() => setBusy(false))
            }}
          >
            <label className={field}>
              {t('booharu.context.source')}
              <Input
                autoFocus
                value={entry.source}
                className='text-[12px]'
                onChange={(event) => setEntry({ ...entry, source: event.currentTarget.value })}
              />
            </label>
            <label className={field}>
              {t('booharu.context.target')}
              <Input
                value={entry.target}
                className='text-[12px]'
                onChange={(event) => setEntry({ ...entry, target: event.currentTarget.value })}
              />
            </label>
            <div className={field}>
              <span id={`${id}-kind`}>{t('booharu.context.kind')}</span>
              <Select
                value={entry.kind}
                items={Object.fromEntries(termKinds.map((kind) => [kind, t(kindName(kind))]))}
                onValueChange={(kind) => {
                  if (kind) setEntry({ ...entry, kind: kind as TermKind })
                }}
              >
                <SelectTrigger aria-labelledby={`${id}-kind`} className='h-8 w-full text-[12px]'>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {termKinds.map((kind) => (
                    <SelectItem key={kind} value={kind}>
                      {t(kindName(kind))}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <label className={field}>
              {t('booharu.context.notes')}
              <Textarea
                value={entry.note}
                className='min-h-16 text-[12px]'
                onChange={(event) => setEntry({ ...entry, note: event.currentTarget.value })}
              />
            </label>
            <label className='flex items-center gap-2 text-[12px]'>
              <Checkbox
                checked={entry.locked}
                onCheckedChange={(locked) => setEntry({ ...entry, locked })}
              />
              {t('booharu.context.lockTerm')}
            </label>
            {replaced && (
              <p className='text-[11px] text-muted-foreground'>
                {t('booharu.context.replaces', {
                  source: replaced.source,
                  target: replaced.target,
                })}
              </p>
            )}
            <DialogFooter>
              <Button type='button' variant='ghost' disabled={busy} onClick={onClose}>
                {t('common.cancel')}
              </Button>
              <Button type='submit' disabled={!complete || busy} aria-busy={busy}>
                {t('booharu.context.save')}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}
