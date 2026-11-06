'use client'

import { useIsMutating, useMutation } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { useKoharuStore } from '@/lib/store'
import { commands, type SecondPassScope } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

import { useChapterContext } from './data'

const mutationKey = ['second-pass'] as const

export function SecondPassButton({ scope }: { scope: SecondPassScope }) {
  const { t } = useTranslation()
  const chapter = useChapterContext().data
  const running = useKoharuStore((state) =>
    Object.values(state.jobs).some((job) => job.state === 'running'),
  )
  const starting = useIsMutating({ mutationKey }) > 0
  const mutation = useMutation({
    mutationKey,
    mutationFn: () => call(commands.startSecondPass, scope),
  })
  return (
    <Button
      size='sm'
      variant='outline'
      className='h-auto min-h-7 min-w-0 whitespace-normal'
      disabled={running || starting || !chapter?.supports_second_pass}
      title={
        !chapter?.supports_second_pass ? t('secondPass.needsModel') : t('secondPass.protected')
      }
      onClick={() => mutation.mutate()}
    >
      {t(`secondPass.${scope.scope === 'boxes' ? 'title' : scope.scope}`)}
    </Button>
  )
}
