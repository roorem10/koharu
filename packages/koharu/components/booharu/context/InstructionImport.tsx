'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { importInstructionGlossary } from '@/lib/booharu/context'
import type { InstructionPreview } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from '@koharu/ui/components/dialog'
import { toast } from '@koharu/ui/components/toast'

/** Import from instructions, Booharu 0.0.4's: the terms on the lines of the translation
 * Instructions as `preview` read them, each with its line and a box, all ticked to start with, and
 * whether to remove the ticked lines from the Instructions. Import selected adds the ticked ones to
 * the series `series` as locked terms, keeping a term it has with their source, says how many it
 * added and skipped, and closes; a failure, as when the Instructions changed since the preview,
 * shows its error, as every failed command's does, and keeps the preview open. */
export function InstructionImportDialog({
  series,
  preview,
  onClose,
}: {
  series: string
  preview: InstructionPreview
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [lines, setLines] = useState(() => preview.terms.map((term) => term.line))
  const [remove, setRemove] = useState(false)
  const [busy, setBusy] = useState(false)

  const run = () => {
    setBusy(true)
    // In the Instructions' order, whatever order they were ticked in.
    const ticked = preview.terms.map((term) => term.line).filter((line) => lines.includes(line))
    importInstructionGlossary(series, ticked, remove, preview.instructions)
      .then((done) => {
        const notes = [
          done.skipped > 0 ? t('booharu.context.skippedTerms', { count: done.skipped }) : null,
          done.instructions !== null ? t('booharu.context.removedLines') : null,
        ].filter((note) => note !== null)
        toast.add({
          type: 'success',
          title: t('booharu.context.importedTerms', { count: done.added }),
          description: notes.join(' ') || undefined,
        })
        onClose()
      })
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
      <DialogContent showCloseButton={false} className='gap-3 sm:max-w-md'>
        <DialogTitle className='text-[13px] font-semibold'>
          {t('booharu.context.importInstructions')}
        </DialogTitle>
        <DialogDescription className='text-[11px]'>
          {preview.terms.length === 0
            ? t('booharu.context.noInstructionTerms')
            : t('booharu.context.importLocked')}
        </DialogDescription>
        {preview.terms.length > 0 && (
          <>
            <ul className='grid max-h-64 gap-1.5 overflow-y-auto text-[12px]'>
              {preview.terms.map((term) => (
                <li key={term.line}>
                  <label className='flex min-w-0 items-start gap-2'>
                    <Checkbox
                      className='mt-0.5'
                      checked={lines.includes(term.line)}
                      onCheckedChange={(checked) =>
                        setLines((held) =>
                          checked
                            ? [...held, term.line]
                            : held.filter((line) => line !== term.line),
                        )
                      }
                    />
                    <span className='min-w-0 flex-1 break-words'>
                      {term.source} → {term.target}
                    </span>
                    {/* A space, which the row doesn't show, keeps the line apart in the box's name. */}{' '}
                    <span className='shrink-0 text-[10px] text-muted-foreground'>
                      {t('booharu.context.lineNumber', { number: term.line + 1 })}
                    </span>
                  </label>
                </li>
              ))}
            </ul>
            <label className='flex items-center gap-2 text-[12px]'>
              <Checkbox checked={remove} onCheckedChange={setRemove} />
              {t('booharu.context.removeImportedLines')}
            </label>
          </>
        )}
        <DialogFooter>
          <Button type='button' variant='ghost' disabled={busy} onClick={onClose}>
            {t('common.cancel')}
          </Button>
          {preview.terms.length > 0 && (
            <Button
              type='button'
              disabled={busy || lines.length === 0}
              aria-busy={busy}
              onClick={run}
            >
              {t('booharu.context.importSelected')}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
