'use client'

import { useTranslation } from 'react-i18next'

import { SeriesPicker } from '@/components/booharu/context/SeriesPicker'
import { TermForm } from '@/components/booharu/context/TermForm'
import {
  changeSeries,
  failure,
  linkSeries,
  putTerm,
  useChapterContext,
  type FullEntry,
} from '@/lib/booharu/context'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from '@koharu/ui/components/dialog'

/** Add to glossary, from a line's text: the term form filled from `initial`, with Lock, saving the
 * term into the series the open chapter follows in place of a term with its source, then showing
 * where it's used, with Re-translate those lines, until Done. While the chapter follows no series,
 * or one that's gone, it asks for one first, as Booharu 0.0.4 did, and links the chapter to the
 * series picked or made; `onClose` closes it. */
export function AddToGlossary({ initial, onClose }: { initial: FullEntry; onClose: () => void }) {
  const { t } = useTranslation()
  const chapter = useChapterContext()
  const linked = chapter.data?.series ?? null

  if (linked) {
    return (
      <TermForm
        initial={initial}
        glossary={linked.glossary}
        showUsage
        onClose={onClose}
        onSave={(entry) => changeSeries(linked.id, (held) => putTerm(held, entry))}
      />
    )
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent showCloseButton={false} className='gap-3 sm:max-w-md'>
        <DialogTitle className='text-[13px] font-semibold'>
          {t('booharu.context.addToGlossary')}
        </DialogTitle>
        <DialogDescription className='text-[11px]'>
          {chapter.data?.missing
            ? t('booharu.context.missingSeries')
            : t('booharu.context.chooseSeriesHint')}
        </DialogDescription>
        {chapter.error ? (
          <div role='alert' className='grid gap-1 text-[11px] leading-4'>
            <p className='font-medium'>{t('booharu.context.loadFailed')}</p>
            <p className='break-words text-muted-foreground'>{failure(chapter.error)}</p>
          </div>
        ) : chapter.data ? (
          <SeriesPicker value={null} onChange={(id) => linkSeries(id)} />
        ) : (
          <p className='text-[11px] text-muted-foreground'>{t('booharu.context.loading')}</p>
        )}
        <DialogFooter>
          <Button type='button' variant='ghost' onClick={onClose}>
            {t('common.cancel')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
