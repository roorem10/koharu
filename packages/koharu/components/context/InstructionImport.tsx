'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call, refreshPreferences } from '@/lib/backend'
import { commands, type InstructionPreview } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import { Dialog, DialogContent, DialogTitle } from '@koharu/ui/components/dialog'

import { refreshContext } from './data'

export function InstructionImport({ series }: { series: string }) {
  const { t } = useTranslation()
  const [preview, setPreview] = useState<InstructionPreview | null>(null)
  const [selected, setSelected] = useState<number[]>([])
  const [remove, setRemove] = useState(false)
  const [busy, setBusy] = useState(false)
  return (
    <>
      <Button
        size='sm'
        variant='outline'
        onClick={() =>
          void call(commands.previewInstructionGlossary)
            .then((data) => {
              setPreview(data)
              setSelected(data.terms.map((term) => term.line))
              setRemove(false)
            })
            .catch(() => undefined)
        }
      >
        {t('chapterContext.importInstructions')}
      </Button>
      <Dialog open={Boolean(preview)} onOpenChange={(open) => !open && !busy && setPreview(null)}>
        <DialogContent showCloseButton={false}>
          <DialogTitle>{t('chapterContext.importInstructions')}</DialogTitle>
          <div className='grid max-h-64 gap-2 overflow-y-auto text-xs'>
            {preview?.terms.length === 0 && <p>{t('chapterContext.noInstructionTerms')}</p>}
            {preview?.terms.map((term) => (
              <label key={term.line} className='flex items-start gap-2 break-words'>
                <Checkbox
                  checked={selected.includes(term.line)}
                  onCheckedChange={(checked) =>
                    setSelected(
                      checked
                        ? [...selected, term.line]
                        : selected.filter((line) => line !== term.line),
                    )
                  }
                />
                <span>
                  {term.source} → {term.target}
                </span>
              </label>
            ))}
          </div>
          <label className='flex gap-2 text-xs'>
            <Checkbox checked={remove} onCheckedChange={(checked) => setRemove(checked)} />
            {t('chapterContext.removeImportedLines')}
          </label>
          <p className='text-xs text-muted-foreground'>{t('chapterContext.importLocked')}</p>
          <div className='flex justify-end gap-2'>
            <Button variant='ghost' disabled={busy} onClick={() => setPreview(null)}>
              {t('chapterContext.cancel')}
            </Button>
            <Button
              disabled={busy || !selected.length}
              onClick={() => {
                if (!preview) return
                setBusy(true)
                void call(
                  commands.importInstructionGlossary,
                  series,
                  preview.instructions,
                  selected,
                  remove,
                )
                  .then(async () => {
                    await refreshContext()
                    if (remove) await refreshPreferences()
                    setPreview(null)
                  })
                  .catch(() => undefined)
                  .finally(() => setBusy(false))
              }}
            >
              {t('chapterContext.importSelected')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  )
}
