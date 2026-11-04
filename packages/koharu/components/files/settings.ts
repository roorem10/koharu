'use client'

import { useIsMutating, useMutation, useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { queryClient } from '@/lib/queries'
import { commands, type LibrarySettings } from '@koharu/bridge/protocol'

export const libraryKey = ['library-settings'] as const

export function useLibrarySettings() {
  const saving = useIsMutating({ mutationKey: libraryKey }) > 0
  const query = useQuery({
    queryKey: libraryKey,
    queryFn: () => call(commands.getLibrarySettings),
  })
  const mutation = useMutation({
    mutationKey: libraryKey,
    scope: { id: 'library-settings' },
    mutationFn: (settings: LibrarySettings) => call(commands.saveLibrarySettings, settings),
    onSuccess: (settings) => {
      queryClient.setQueryData(libraryKey, settings)
    },
  })
  return { ...query, save: mutation.mutateAsync, saving }
}
