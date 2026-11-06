'use client'

import { useMutation } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { projectKey, refresh } from '@/lib/queries'
import { commands, type TermConflict, type TermConflictChoice } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

import { refreshContext } from './data'

export function SeriesTermSync({ conflicts }: { conflicts: TermConflict[] }) {
  const { t } = useTranslation()
  const [saved, setSaved] = useState<number | null>(null)
  const mutation = useMutation({
    mutationFn: async (resolution?: { conflict: TermConflict; choice: TermConflictChoice }) => {
      setSaved(null)
      if (resolution) {
        await call(commands.resolveSeriesTermConflict, resolution.conflict, resolution.choice)
      } else {
        const result = await call(commands.saveChapterTermsToSeries)
        setSaved(result.added)
      }
      await Promise.all([refreshContext(), refresh(projectKey)])
    },
  })
  return (
    <div className='grid min-w-0 gap-2'>
      <Button
        size='sm'
        variant='outline'
        className='h-auto min-h-8 whitespace-normal'
        disabled={mutation.isPending}
        onClick={() => mutation.mutate(undefined)}
      >
        {t('chapterContext.saveTermsToSeries')}
      </Button>
      <p className='text-muted-foreground'>{t('chapterContext.saveUsedTermsHint')}</p>
      {saved !== null && <p role='status'>{t('chapterContext.termsSaved', { count: saved })}</p>}
      {conflicts.length > 0 && (
        <section className='grid min-w-0 gap-2' aria-label={t('chapterContext.termConflicts')}>
          <h4 className='font-semibold'>{t('chapterContext.termConflicts')}</h4>
          {conflicts.map((conflict) => (
            <div
              key={`${conflict.source}:${conflict.series_target}:${conflict.chapter_target}`}
              className='grid min-w-0 gap-1 rounded-md border p-2'
            >
              <p className='font-medium break-words'>{conflict.source}</p>
              <p className='break-words'>
                {t('chapterContext.seriesTerm', { target: conflict.series_target })}
              </p>
              <p className='break-words'>
                {t('chapterContext.chapterTerm', { target: conflict.chapter_target })}
              </p>
              <div className='flex flex-wrap gap-1'>
                {(['keep_series', 'use_chapter', 'leave'] as const).map((choice) => (
                  <Button
                    key={choice}
                    size='sm'
                    variant='outline'
                    className='h-auto min-h-8 max-w-full whitespace-normal'
                    disabled={mutation.isPending}
                    onClick={() => mutation.mutate({ conflict, choice })}
                  >
                    {t(`chapterContext.termConflictChoices.${choice}`)}
                  </Button>
                ))}
              </div>
            </div>
          ))}
        </section>
      )}
    </div>
  )
}
