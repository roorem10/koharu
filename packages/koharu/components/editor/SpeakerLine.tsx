'use client'

import { useMutation } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { refreshContext, useChapterContext } from '@/components/context/data'
import { call } from '@/lib/backend'
import { projectKey, refresh } from '@/lib/queries'
import { commands, type EntityId } from '@koharu/bridge/protocol'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

export function SpeakerLine({ layer }: { layer: EntityId }) {
  const { t } = useTranslation()
  const chapter = useChapterContext()
  const characters = chapter.data?.series?.characters ?? []
  const speaker = chapter.data?.context.lines[layer]?.speaker
  const character = characters.find((character) => character.id === speaker?.value)
  const save = useMutation({
    mutationFn: (speaker: string | null) => call(commands.setBoxSpeaker, layer, speaker),
    onSuccess: async () => {
      await Promise.all([refreshContext(), refresh(projectKey)])
    },
  })
  return (
    <div className='grid min-w-0 gap-1'>
      <span className='text-[11px] text-muted-foreground'>{t('boxSpeaker.label')}</span>
      <Select
        value={
          character?.id ?? (speaker?.source === 'user' && speaker.value === null ? 'none' : null)
        }
        disabled={chapter.isPending || chapter.isError || save.isPending}
        onValueChange={(value, details) => {
          if (value !== null && details.reason === 'item-press') {
            save.mutate(value === 'none' ? null : value)
          }
        }}
      >
        <SelectTrigger size='sm' className='w-full min-w-0' aria-label={t('boxSpeaker.label')}>
          <SelectValue className='min-w-0'>
            <span className='truncate'>{character?.name ?? t('boxSpeaker.none')}</span>
          </SelectValue>
        </SelectTrigger>
        <SelectContent alignItemWithTrigger={false}>
          <SelectItem value='none'>{t('boxSpeaker.none')}</SelectItem>
          {characters.map((character) => (
            <SelectItem key={character.id} value={character.id}>
              <span className='break-words whitespace-normal'>{character.name}</span>
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {speaker && (
        <span className='text-[10px] leading-4 text-muted-foreground'>
          {t(`lettering.reasons.${speaker.source}`)}
        </span>
      )}
      {(chapter.isError || save.isError) && (
        <span role='alert' className='text-[10px]'>
          {t(chapter.isError ? 'boxSpeaker.loadError' : 'boxSpeaker.saveError')}
        </span>
      )}
    </div>
  )
}
