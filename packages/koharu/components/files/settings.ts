'use client'

import { useMutation, useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { queryClient } from '@/lib/queries'
import { commands, type LibrarySettings } from '@koharu/bridge/protocol'

const libraryKey = ['library-settings'] as const

export function useLibrarySettings() {
  const query = useQuery({
    queryKey: libraryKey,
    queryFn: () => call(commands.getLibrarySettings),
  })
  const mutation = useMutation({
    mutationFn: (settings: LibrarySettings) => call(commands.saveLibrarySettings, settings),
    onSuccess: (settings) => { queryClient.setQueryData(libraryKey, settings) },
  })
  return { ...query, save: mutation.mutateAsync, saving: mutation.isPending }
}
