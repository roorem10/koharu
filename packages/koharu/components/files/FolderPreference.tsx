'use client'

import { useMutation } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { queryClient } from '@/lib/queries'
import { commands, type LibraryFolder } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

import { libraryKey, useLibrarySettings } from './settings'

export function FolderPreference({
  kind,
  active,
  defaultPath,
  selected,
}: {
  kind: LibraryFolder
  active: string | undefined
  defaultPath: string | undefined
  selected: string | null | undefined
}) {
  const { t } = useTranslation()
  const settings = useLibrarySettings()
  const change = useMutation({
    mutationKey: libraryKey,
    scope: { id: 'library-settings' },
    mutationFn: async (action: 'choose' | 'reset' | 'open') => {
      if (action === 'choose') return call(commands.chooseLibraryFolder, kind)
      if (action === 'reset') return call(commands.resetLibraryFolder, kind)
      await call(commands.openLibraryFolder, kind)
      return null
    },
    onSuccess: (value) => {
      if (value) queryClient.setQueryData(libraryKey, value)
    },
  })
  const desired = selected ?? defaultPath
  const disabled = settings.saving || !settings.data || !active
  return (
    <div className='grid min-w-0 gap-2'>
      <p className='text-[11px] break-all'>{active ?? t('files.loading')}</p>
      {defaultPath && (
        <p className='text-[10px] break-all text-muted-foreground'>
          {t('files.defaultFolder', { path: defaultPath })}
        </p>
      )}
      {desired && desired !== active && (
        <p className='text-[10px] break-all text-muted-foreground'>
          {t('files.nextLaunch', { path: desired })}
        </p>
      )}
      <div className='flex flex-wrap gap-1.5'>
        <Button
          size='sm'
          variant='outline'
          disabled={disabled}
          onClick={() => change.mutate('choose')}
        >
          {t('files.changeFolder')}
        </Button>
        <Button
          size='sm'
          variant='outline'
          disabled={disabled || selected === null}
          onClick={() => change.mutate('reset')}
        >
          {t('files.useDefault')}
        </Button>
        <Button
          size='sm'
          variant='outline'
          disabled={disabled}
          onClick={() => change.mutate('open')}
        >
          {t('files.openFolder')}
        </Button>
      </div>
    </div>
  )
}
