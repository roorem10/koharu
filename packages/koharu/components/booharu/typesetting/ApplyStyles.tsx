'use client'

import { useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { PreferenceRow } from '@/components/preferences/PreferenceFields'
import { call } from '@/lib/backend'
import { useProject } from '@/lib/queries'
import { commands, type StyleCounts, type StyleScope } from '@koharu/ipc/protocol'
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
import { Checkbox } from '@koharu/ui/components/checkbox'
import { toast } from '@koharu/ui/components/toast'

const PROJECT: StyleScope = { kind: 'project' }

/** What the desktop counted, and whether the boxes changed by hand were included. */
type Counted = { include: boolean; counts: StyleCounts }

/** Apply to the open project, the default text style's last row: it counts the boxes the style
 * would restyle, keep and find untranslated, and asks with those numbers, counting again when the
 * boxes changed by hand are included; then it applies the style as one undo step and says how many
 * boxes it restyled and kept. The pages it changes arrive as every change does. */
export function ApplyStyles() {
  const { t } = useTranslation()
  const project = useProject().data
  const [open, setOpen] = useState(false)
  const [opening, setOpening] = useState(false)
  const [applying, setApplying] = useState(false)
  const [include, setInclude] = useState(false)
  // The last count, kept while the dialog closes so its numbers don't change as it fades. It's
  // current while it includes what the box says; a count asked for since is still to come.
  const [counted, setCounted] = useState<Counted | null>(null)
  /** Numbers each count, so only the last one asked for shows. */
  const asked = useRef(0)

  const start = () => {
    const ask = ++asked.current
    setOpening(true)
    call(commands.booharuCountTextStyleTargets, PROJECT, false)
      .then((counts) => {
        if (ask !== asked.current) return
        setInclude(false)
        setCounted({ include: false, counts })
        setOpen(true)
      })
      .catch(() => undefined)
      .finally(() => setOpening(false))
  }

  const recount = (next: boolean) => {
    const ask = ++asked.current
    const shown = counted
    setInclude(next)
    call(commands.booharuCountTextStyleTargets, PROJECT, next).then(
      (counts) => {
        if (ask === asked.current) setCounted({ include: next, counts })
      },
      // The box goes back to what the numbers shown include.
      () => {
        if (ask === asked.current && shown) setInclude(shown.include)
      },
    )
  }

  /** The line saying how many boxes `counts` keeps: changed by hand, or, with those included,
   * locked. */
  const kept = ({ include: included, counts }: Counted) =>
    counts.kept_hand_edited > 0
      ? t(included ? 'booharu.typesetting.apply.keptLocked' : 'booharu.typesetting.apply.kept', {
          count: counts.kept_hand_edited,
        })
      : null

  const apply = () => {
    if (!counted) return
    const included = counted.include
    setApplying(true)
    call(commands.booharuApplyTextStyles, PROJECT, included)
      .then((done) => {
        setOpen(false)
        toast.add({
          type: 'success',
          title: t('booharu.typesetting.apply.restyled', { count: done.changed }),
          description: kept({ include: included, counts: done }) ?? undefined,
        })
      })
      .catch(() => undefined)
      .finally(() => setApplying(false))
  }

  const current = counted !== null && counted.include === include
  const counts = counted?.counts
  const keptLine = counted && kept(counted)

  return (
    <>
      <PreferenceRow
        title={t('booharu.typesetting.apply.row')}
        description={t('booharu.typesetting.apply.description', {
          setting: t('booharu.pipeline.detectColors'),
        })}
      >
        <Button
          type='button'
          size='sm'
          variant='outline'
          disabled={!project || opening}
          aria-busy={opening}
          onClick={start}
        >
          {t('booharu.typesetting.apply.title')}
        </Button>
      </PreferenceRow>
      <AlertDialog
        open={open}
        onOpenChange={(next) => {
          if (!next && !applying) setOpen(false)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('booharu.typesetting.apply.title')}</AlertDialogTitle>
            <AlertDialogDescription render={<div />} aria-busy={!current} className='grid gap-1'>
              {counts && (
                <>
                  <p>{t('booharu.typesetting.apply.restyle', { count: counts.changed })}</p>
                  {keptLine && <p>{keptLine}</p>}
                  {counts.untranslated > 0 && (
                    <p>
                      {t('booharu.typesetting.apply.untranslated', { count: counts.untranslated })}
                    </p>
                  )}
                </>
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <label className='flex items-center gap-2 text-sm'>
            <Checkbox
              checked={include}
              disabled={applying}
              onCheckedChange={(next) => recount(next)}
            />
            {t('booharu.typesetting.apply.includeHandEdited')}
          </label>
          <p className='text-xs text-muted-foreground'>{t('booharu.typesetting.apply.undo')}</p>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={applying}>{t('common.cancel')}</AlertDialogCancel>
            <AlertDialogAction
              disabled={!current || applying || !counts?.changed}
              aria-busy={applying}
              onClick={apply}
            >
              {t('booharu.typesetting.apply.confirm')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
