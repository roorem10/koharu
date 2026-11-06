'use client'

import { useQuery } from '@tanstack/react-query'
import { create } from 'zustand'

import { useContextSettings, useChapterContext } from '@/components/context/data'
import { call } from '@/lib/backend'
import { queryClient, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/bridge/protocol'

export const readyCheckKey = ['ready-check'] as const

export const useReadyCheckDialog = create<{
  isOpen: boolean
  open: () => void
  close: () => void
}>()((set) => ({
  isOpen: false,
  open: () => set({ isOpen: true }),
  close: () => set({ isOpen: false }),
}))

export type ReadySettingsTab = 'translation' | 'typesetting' | 'pipeline' | 'providers'
export const useReadySettings = create<{
  target: { tab: ReadySettingsTab } | null
  open: (tab: ReadySettingsTab) => void
}>()((set) => ({
  target: null,
  open: (tab) => {
    useReadyCheckDialog.getState().close()
    set({ target: { tab } })
    useKoharuStore.getState().setSettingsOpen(true)
  },
}))

export function refreshReadyCheck() {
  return queryClient.invalidateQueries({ queryKey: readyCheckKey })
}

/** A single shared query serves Settings, the dialog, and the non-modal indicator. */
export function useReadyCheck() {
  const project = useProject().data
  const preferences = useKoharuStore((state) => state.preferences)
  const textStyles = useKoharuStore((state) => state.textStyles)
  const downloads = useKoharuStore((state) =>
    Object.values(state.downloads)
      .map((download) => `${download.id}:${download.state}`)
      .join(','),
  )
  const context = useChapterContext()
  const settings = useContextSettings()
  return useQuery({
    queryKey: [
      ...readyCheckKey,
      project?.name,
      project?.revision,
      preferences,
      textStyles,
      context.data?.context.series,
      settings.data,
      downloads,
    ],
    queryFn: () => call(commands.getReadyCheck),
    staleTime: 5_000,
    refetchInterval: 30_000,
  })
}
