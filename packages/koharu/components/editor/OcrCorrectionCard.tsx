'use client'

import { useMutation, useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { refreshContext, useChapterContext } from '@/components/context/data'
import { call } from '@/lib/backend'
import { pageKey, projectKey, queryClient, refresh, useProject } from '@/lib/queries'
import { commands, type EntityId } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

const suggestionsKey = ['ocr-corrections'] as const

export function OcrCorrectionCard({ layer }: { layer: EntityId }) {
  const { t } = useTranslation()
  const project = useProject().data
  const chapter = useChapterContext()
  const report = useQuery({
    queryKey: [...suggestionsKey, layer, project?.revision, chapter.dataUpdatedAt],
    queryFn: () => call(commands.getOcrSuggestions, { scope: 'entities', value: [layer] }),
    enabled: Boolean(project) && chapter.dataUpdatedAt > 0,
  }).data
  const mutation = useMutation({
    mutationFn: (action: 'accept' | 'dismiss') =>
      action === 'accept'
        ? call(commands.acceptOcrCorrection, layer)
        : call(commands.dismissOcrCorrection, layer),
    onSuccess: async () => {
      await Promise.all([
        refreshContext(),
        refresh(projectKey, pageKey),
        queryClient.invalidateQueries({ queryKey: suggestionsKey }),
      ])
    },
  })
  const suggestion = report?.suggestions.find((item) => item.layer === layer)
  if (!suggestion || !['pending', 'applied'].includes(suggestion.status)) {
    return report?.skip_reason === 'no_vision_model' ? (
      <p className='text-muted-foreground'>{t('ocrCorrections.noVisionModel')}</p>
    ) : null
  }
  const applied = suggestion.status === 'applied'
  return (
    <section
      aria-label={t('ocrCorrections.title')}
      className='grid min-w-0 gap-2 rounded-md border bg-muted/30 p-2'
    >
      <p className='font-medium'>
        {t(applied ? 'ocrCorrections.corrected' : 'ocrCorrections.title')}
      </p>
      <dl className='grid min-w-0 gap-2'>
        <div>
          <dt className='text-muted-foreground'>{t('ocrCorrections.original')}</dt>
          <dd className='break-words whitespace-pre-wrap'>{suggestion.original}</dd>
        </div>
        <div>
          <dt className='text-muted-foreground'>{t('ocrCorrections.suggested')}</dt>
          <dd className='break-words whitespace-pre-wrap'>{suggestion.suggested}</dd>
        </div>
        <div>
          <dt className='text-muted-foreground'>{t('ocrCorrections.reason')}</dt>
          <dd className='break-words whitespace-pre-wrap'>{suggestion.reason}</dd>
        </div>
      </dl>
      {!applied && (
        <div className='flex flex-wrap gap-2'>
          <Button size='sm' disabled={mutation.isPending} onClick={() => mutation.mutate('accept')}>
            {t('ocrCorrections.accept')}
          </Button>
          <Button
            size='sm'
            variant='ghost'
            disabled={mutation.isPending}
            onClick={() => mutation.mutate('dismiss')}
          >
            {t('chapterContext.dismiss')}
          </Button>
        </div>
      )}
    </section>
  )
}
