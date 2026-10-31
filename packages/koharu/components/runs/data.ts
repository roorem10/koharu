'use client'

import { useMutation, useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { pageKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type Job,
  type RunSettings,
  type Scope,
  type ProjectInfo,
} from '@koharu/bridge/protocol'

import { prepareFinishSound } from './sound'

export async function retryFailedPages(job: Job) {
  prepareFinishSound()
  await call(commands.retryFailedPages, job.id)
}

export async function openFailedPage(page: string) {
  const selection = await call(commands.selectPage, page)
  queryClient.setQueryData(projectKey, selection.project)
  queryClient.setQueryData(pageKey, selection.page)
  useKoharuStore.getState().selectPages([page])
  useKoharuStore.getState().selectLayers([])
}

// Defaulted input fields are optional in the bindings; native responses serialize every field.
export type SavedRunSettings = Required<RunSettings>

export const runSettingsKey = ['run-settings'] as const

export function useRunSettings() {
  return useQuery({
    queryKey: runSettingsKey,
    queryFn: async () => (await call(commands.getRunSettings)) as SavedRunSettings,
  })
}

export function useSaveRunSettings() {
  return useMutation({
    mutationFn: (settings: SavedRunSettings) => call(commands.saveRunSettings, settings),
    onSuccess: (settings) => {
      queryClient.setQueryData(runSettingsKey, settings as SavedRunSettings)
    },
  })
}

export function currentRunScope(): Scope | null {
  const state = useKoharuStore.getState()
  switch (state.processingScope) {
    case 'project':
      return { scope: 'project' }
    case 'selected-pages':
      return state.selectedPages.length ? { scope: 'pages', value: state.selectedPages } : null
    case 'page': {
      const page =
        queryClient.getQueryData<ProjectInfo>(projectKey)?.active_page ?? state.canvasPage
      return page ? { scope: 'pages', value: [page] } : null
    }
  }
}

export function presetLabel(name: string, t: (key: string) => string) {
  if (name === 'Detect + OCR') return t('runs.detectOcr')
  if (name === 'Translate + clean up') return t('runs.translateCleanup')
  return name
}
