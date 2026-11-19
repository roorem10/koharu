'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { answerRedo, useRedo, type PendingRedo } from '@/lib/booharu/redo'
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

/** The confirmation Redo boxes opens before Read or Translate replaces what was fixed by hand: how
 * many of the boxes it would overwrite, and Overwrite and run, which runs on every box; Skip those,
 * which runs on the others, offered only while there are others; and Cancel, which runs nothing,
 * as Escape does. Mounted once, in the editor, where every box action is. */
export function RedoDialog() {
  const { t } = useTranslation()
  const pending = useRedo((state) => state.pending)
  // The redo last asked about, whose text the dialog keeps while it closes.
  const [last, setLast] = useState<PendingRedo | null>(pending)
  if (pending && pending !== last) setLast(pending)
  const shown = pending ?? last

  return (
    <AlertDialog
      open={pending !== null}
      onOpenChange={(open) => {
        if (!open) answerRedo('cancel')
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t('booharu.redo.title')}</AlertDialogTitle>
          <AlertDialogDescription>
            {shown && t(`booharu.redo.${shown.stage}`, { count: shown.counts.hand_fixed })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{t('common.cancel')}</AlertDialogCancel>
          {shown && shown.counts.ready > 0 && (
            <AlertDialogAction variant='outline' onClick={() => answerRedo('skip')}>
              {t('booharu.redo.skip')}
            </AlertDialogAction>
          )}
          <AlertDialogAction onClick={() => answerRedo('overwrite')}>
            {t('booharu.redo.overwrite')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
