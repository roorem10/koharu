'use client'

import { BookOpen } from 'lucide-react'
import { useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { GlossaryDialog } from '@/components/booharu/context/GlossaryDialog'
import { dismissFirstStartNote, useFirstStartNote } from '@/lib/booharu/context'
import { Button } from '@koharu/ui/components/button'

/** The note on the home page until it's dismissed (D20): Booharu 0.0.5 starts with fresh settings,
 * and Booharu 0.0.4's glossaries come back with Import series in the glossary editor, which it
 * opens, from the folder it names: 0.0.4's series beside the running program when they're there,
 * else where they'd be beside 0.0.4's Booharu.exe. Dismiss saves `[booharu-start]
 * fresh_start_note_seen`; a failure shows its error, as every failed command's does, and keeps the
 * note. */
export function FirstStartNote() {
  const { t } = useTranslation()
  const heading = useId()
  const note = useFirstStartNote().data
  const [busy, setBusy] = useState(false)
  const [glossaryOpen, setGlossaryOpen] = useState(false)

  if (!note?.show) return null
  return (
    <section
      aria-labelledby={heading}
      className='mt-6 grid gap-2 rounded-2xl border border-border/80 bg-[var(--surface-panel)] p-5 text-[11px] leading-5'
    >
      <h2 id={heading} className='text-[13px] font-semibold tracking-[-0.01em]'>
        {t('booharu.start.title')}
      </h2>
      <p className='text-muted-foreground'>{t('booharu.start.body')}</p>
      <p className='break-words text-muted-foreground'>
        {note.folder
          ? t('booharu.start.folder', { folder: note.folder })
          : t('booharu.start.folderUnknown')}
      </p>
      <div className='mt-1 flex flex-wrap gap-2'>
        <Button
          type='button'
          size='sm'
          variant='outline'
          className='h-8 text-[11px]'
          onClick={() => setGlossaryOpen(true)}
        >
          <BookOpen /> {t('booharu.context.openGlossary')}
        </Button>
        <Button
          type='button'
          size='sm'
          className='h-8 text-[11px]'
          disabled={busy}
          aria-busy={busy}
          onClick={() => {
            setBusy(true)
            dismissFirstStartNote()
              .catch(() => undefined)
              .finally(() => setBusy(false))
          }}
        >
          {t('booharu.start.dismiss')}
        </Button>
      </div>
      {glossaryOpen && <GlossaryDialog initial={null} onClose={() => setGlossaryOpen(false)} />}
    </section>
  )
}
