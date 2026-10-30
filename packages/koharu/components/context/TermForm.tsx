'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { TermKind } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import { Dialog, DialogContent, DialogTitle } from '@koharu/ui/components/dialog'
import { Input } from '@koharu/ui/components/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'
import { Textarea } from '@koharu/ui/components/textarea'

import type { GlossaryEntry } from './types'

const kinds: TermKind[] = ['name', 'place', 'term', 'honorific', 'other']

export function TermForm({
  initial,
  onSave,
  onClose,
}: {
  initial: GlossaryEntry
  onSave: (entry: GlossaryEntry) => Promise<void>
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [entry, setEntry] = useState(initial)
  const [busy, setBusy] = useState(false)
  return (
    <Dialog open onOpenChange={(open) => !open && !busy && onClose()}>
      <DialogContent showCloseButton={false}>
        <DialogTitle>{t('chapterContext.editTerm')}</DialogTitle>
        <form
          className='grid gap-3'
          onSubmit={(event) => {
            event.preventDefault()
            setBusy(true)
            void onSave({ ...entry, source: entry.source.trim(), target: entry.target.trim() })
              .then(onClose)
              .catch(() => undefined)
              .finally(() => setBusy(false))
          }}
        >
          <label className='grid gap-1 text-xs'>
            {t('chapterContext.source')}
            <Input
              autoFocus
              value={entry.source}
              onChange={(event) => setEntry({ ...entry, source: event.target.value })}
              required
            />
          </label>
          <label className='grid gap-1 text-xs'>
            {t('chapterContext.target')}
            <Input
              value={entry.target}
              onChange={(event) => setEntry({ ...entry, target: event.target.value })}
              required
            />
          </label>
          <Select
            value={entry.kind}
            items={Object.fromEntries(
              kinds.map((kind) => [kind, t(`chapterContext.kinds.${kind}`)]),
            )}
            onValueChange={(value) => value && setEntry({ ...entry, kind: value as TermKind })}
          >
            <SelectTrigger aria-label={t('chapterContext.kind')}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {kinds.map((kind) => (
                <SelectItem key={kind} value={kind}>
                  {t(`chapterContext.kinds.${kind}`)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <label className='grid gap-1 text-xs'>
            {t('chapterContext.notes')}
            <Textarea
              value={entry.note}
              onChange={(event) => setEntry({ ...entry, note: event.target.value })}
            />
          </label>
          <label className='flex items-center gap-2 text-xs'>
            <Checkbox
              checked={entry.locked}
              onCheckedChange={(checked) => setEntry({ ...entry, locked: checked })}
            />
            {t('chapterContext.lockTerm')}
          </label>
          <div className='flex justify-end gap-2'>
            <Button type='button' variant='ghost' disabled={busy} onClick={onClose}>
              {t('chapterContext.cancel')}
            </Button>
            <Button type='submit' disabled={busy || !entry.source.trim() || !entry.target.trim()}>
              {t('chapterContext.save')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export function newTerm(source = '', target = ''): GlossaryEntry {
  return { id: crypto.randomUUID(), source, target, note: '', kind: 'term', locked: false }
}
