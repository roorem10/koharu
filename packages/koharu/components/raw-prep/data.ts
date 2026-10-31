'use client'

import { useIsMutating, useMutation, useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { create } from 'zustand'

import { call } from '@/lib/backend'
import { pageKey, pagesKey, projectKey, queryClient, refresh } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type EntityId,
  type RawPrepOptions,
  type RawPrepSettings,
} from '@koharu/bridge/protocol'
import { toast } from '@koharu/ui/components/toast'

export type PrepOptions = Required<RawPrepOptions>
export type PrepSettings = Omit<Required<RawPrepSettings>, 'options'> & { options: PrepOptions }

export const rawPrepSettingsKey = ['raw-prep-settings'] as const
const rawPrepMutationKey = ['raw-prep'] as const

export const usePrepDialog = create<{
  project: string | null
  open: (project: string) => void
  close: () => void
}>()((set) => ({
  project: null,
  open: (project) => set({ project }),
  close: () => set({ project: null }),
}))

export function useRawPrepSettings() {
  return useQuery({
    queryKey: rawPrepSettingsKey,
    queryFn: async () => (await call(commands.getRawPrepSettings)) as PrepSettings,
  })
}

export function useSaveRawPrepSettings() {
  return useMutation({
    mutationKey: rawPrepSettingsKey,
    scope: { id: 'raw-prep-settings' },
    mutationFn: (settings: PrepSettings) => call(commands.saveRawPrepSettings, settings),
    onSuccess: (settings) => queryClient.setQueryData(rawPrepSettingsKey, settings as PrepSettings),
  })
}

export function useRawPrepBusy() {
  const running = useKoharuStore((state) =>
    Object.values(state.jobs).some((job) => job.state === 'running'),
  )
  return useIsMutating({ mutationKey: rawPrepMutationKey }) > 0 || running
}

export function usePreparePages() {
  return useMutation({
    mutationKey: rawPrepMutationKey,
    meta: { activity: 'rawPrep.preparing' },
    mutationFn: ({ pages, options }: { pages: EntityId[]; options: PrepOptions }) =>
      call(commands.preparePages, pages, options),
    onSuccess: () => refresh(projectKey, pagesKey, pageKey, ['raw-prep-status']),
  })
}

export function useRestorePages() {
  const { t } = useTranslation()
  return useMutation({
    mutationKey: rawPrepMutationKey,
    meta: { activity: 'rawPrep.restoring' },
    mutationFn: async (pages: EntityId[]) => {
      const result = await call(commands.restoreOriginalPages, pages)
      await refresh(projectKey, pagesKey, pageKey, ['raw-prep-status'])
      return result
    },
    onSuccess: (result) => {
      toast.add({
        type: 'info',
        title: t('rawPrep.restored', { count: result.changed }),
        description: result.skipped
          .map((page) =>
            t('rawPrep.skipDetail', {
              page: page.label,
              reason: t(`rawPrep.reason.${page.reason}`),
            }),
          )
          .join('\n'),
      })
    },
  })
}
