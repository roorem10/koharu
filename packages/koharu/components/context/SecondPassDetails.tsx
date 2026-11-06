'use client'

import { useMutation, useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { pageKey, projectKey, refresh, useProject } from '@/lib/queries'
import { commands, type EntityId } from '@koharu/bridge/protocol'
import { Badge } from '@koharu/ui/components/badge'
import { Button } from '@koharu/ui/components/button'

import { refreshContext } from './data'

export function SecondPassDetails({
  layer,
  translation,
}: {
  layer: EntityId
  translation: string
}) {
  const { t } = useTranslation()
  const project = useProject().data
  const result = useQuery({
    queryKey: ['second-pass-result', project?.name, project?.revision, layer],
    queryFn: () => call(commands.getSecondPass, layer),
    enabled: Boolean(project),
  }).data
  const choose = useMutation({
    mutationFn: (text: string) => call(commands.setTranslation, layer, text),
    onSuccess: () => Promise.all([refresh(projectKey, pageKey), refreshContext()]),
  })
  if (!result) return null
  return (
    <div className='grid min-w-0 gap-2 rounded-md border bg-muted/30 p-2'>
      <Badge variant='outline' className='w-fit'>
        {t('secondPass.title')}
      </Badge>
      <p className='break-words whitespace-pre-wrap'>{result.best}</p>
      {result.best !== translation && (
        <Button
          size='sm'
          variant='outline'
          disabled={choose.isPending}
          onClick={() => choose.mutate(result.best)}
        >
          {t('chapterContext.useThis')}
        </Button>
      )}
      <p className='break-words whitespace-pre-wrap'>{result.reason}</p>
      <div className='grid min-w-0 gap-1 border-t pt-2'>
        <p className='text-muted-foreground'>{t('secondPass.previous')}</p>
        <p className='break-words whitespace-pre-wrap'>{result.previous}</p>
        <Button
          size='sm'
          variant='outline'
          className='h-auto min-h-7 whitespace-normal'
          disabled={choose.isPending}
          onClick={() => choose.mutate(result.previous)}
        >
          {t('secondPass.restore')}
        </Button>
      </div>
      {result.alternatives.map((text, index) => (
        <div className='grid min-w-0 gap-1 border-t pt-2' key={index}>
          <p className='break-words whitespace-pre-wrap'>{text}</p>
          <Button
            size='sm'
            variant='outline'
            disabled={choose.isPending}
            onClick={() => choose.mutate(text)}
          >
            {t('chapterContext.useThis')}
          </Button>
        </div>
      ))}
    </div>
  )
}
