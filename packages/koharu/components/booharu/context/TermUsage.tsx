'use client'

import { X } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { failure, fixTermEverywhere, useGlossaryUsage, type FullEntry } from '@/lib/booharu/context'
import { useProject } from '@/lib/queries'
import type { FixedTerm } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'

/** Where a term just saved is used in the open project, as Booharu 0.0.4's TermUsage showed it
 * after a save: on how many lines, how many of them are fixed by hand or locked, which re-translating
 * leaves alone (D1), and Re-translate those lines, Fix it everywhere, which translates the others
 * again, untranslated ones included, in one Translation run following the glossary as it is now,
 * then says what it did and what it left alone. With nothing to translate it starts no run, and
 * says why. It's read again after every change to a page, and shows nothing while no project is
 * open; `onClose`, when given, puts it away. It scrolls into view as it shows, and once more as its
 * counts come, as the list it goes with may have scrolled it away. */
export function TermUsage({ entry, onClose }: { entry: FullEntry; onClose?: () => void }) {
  const { t } = useTranslation()
  const project = useProject().data?.name ?? null
  // As saved: the form saves a term trimmed.
  const source = entry.source.trim()
  const usage = useGlossaryUsage(source)
  const [busy, setBusy] = useState(false)
  const [fixed, setFixed] = useState<FixedTerm | null>(null)
  const box = useRef<HTMLDivElement>(null)
  const counted = usage.data !== undefined
  useEffect(() => box.current?.scrollIntoView({ block: 'nearest' }), [counted])

  if (project === null) return null
  const data = usage.data ?? null

  const fix = () => {
    setBusy(true)
    fixTermEverywhere(source)
      .then(setFixed)
      .catch(() => undefined)
      .finally(() => setBusy(false))
  }

  return (
    <div
      ref={box}
      role='group'
      aria-label={t('booharu.context.usage', { source })}
      className='grid min-w-0 gap-1.5 rounded-md bg-muted p-2 text-[11px] leading-4'
    >
      <div className='flex min-w-0 items-start gap-1'>
        <p className='min-w-0 flex-1 font-medium break-words'>
          {source} → {entry.target.trim()}
        </p>
        {onClose && (
          <Button
            type='button'
            size='icon-xs'
            variant='ghost'
            className='-my-0.5 shrink-0'
            aria-label={t('booharu.context.close')}
            onClick={onClose}
          >
            <X />
          </Button>
        )}
      </div>
      {usage.error ? (
        <div role='alert' className='grid gap-0.5'>
          <p>{t('booharu.context.usageFailed')}</p>
          <p className='break-words text-muted-foreground'>{failure(usage.error)}</p>
        </div>
      ) : data === null ? (
        <p className='text-muted-foreground'>{t('common.loading')}</p>
      ) : (
        <>
          <p>{t('booharu.context.usedOnLines', { count: data.blocks.length })}</p>
          {data.hand_edited > 0 && (
            <p className='text-muted-foreground'>
              {t('booharu.context.handEditsSkipped', { count: data.hand_edited })}
            </p>
          )}
          {data.locked > 0 && (
            <p className='text-muted-foreground'>
              {t('booharu.context.lockedSkipped', { count: data.locked })}
            </p>
          )}
          {/* Why there's nothing to do, which a line used nowhere already says. */}
          {data.blocks.length > 0 && data.translate.length === 0 && fixed === null && (
            <p className='text-muted-foreground'>{t('booharu.context.fixNothing')}</p>
          )}
          <Button
            type='button'
            size='sm'
            variant='outline'
            className='justify-self-start text-[11px]'
            disabled={busy || data.translate.length === 0}
            aria-busy={busy}
            onClick={fix}
          >
            {t('booharu.context.retranslate')}
          </Button>
        </>
      )}
      {fixed && (
        <div role='status' className='grid gap-0.5'>
          {fixed.run !== null ? (
            <p>{t('booharu.context.fixQueued', { count: fixed.translated })}</p>
          ) : (
            <p>
              {fixed.kept_hand_edited + fixed.kept_locked > 0
                ? t('booharu.context.fixNothing')
                : t('booharu.context.fixNoLines')}
            </p>
          )}
          {fixed.kept_hand_edited > 0 && (
            <p className='text-muted-foreground'>
              {t('booharu.context.fixKeptHandEdited', { count: fixed.kept_hand_edited })}
            </p>
          )}
          {fixed.kept_locked > 0 && (
            <p className='text-muted-foreground'>
              {t('booharu.context.fixKeptLocked', { count: fixed.kept_locked })}
            </p>
          )}
        </div>
      )}
    </div>
  )
}
