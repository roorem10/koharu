'use client'

import { useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { invalidateLineLabels } from '@/lib/line-labels'
import { pageKey, projectKey, queryClient, refresh, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type EntityId, type Flag } from '@koharu/bridge/protocol'

import type { ChapterState, Series, ContextSettings } from './types'

export const contextKey = ['chapter-context'] as const
export const seriesKey = ['context-series'] as const
export const contextSettingsKey = ['context-settings'] as const

export function useContextSettings() {
  return useQuery({
    queryKey: contextSettingsKey,
    queryFn: async () => (await call(commands.getContextSettings)) as ContextSettings,
  })
}

export function useChapterContext() {
  const project = useProject().data
  const jobs = useKoharuStore((state) => state.jobs)
  const model = useKoharuStore((state) => state.preferences?.pipeline.translation.model.provider)
  return useQuery({
    queryKey: [
      ...contextKey,
      project?.name,
      project?.revision,
      model,
      Object.values(jobs)
        .map((job) => `${job.id}:${job.state}`)
        .join(','),
    ],
    queryFn: async () => (await call(commands.getChapterContext)) as ChapterState,
    enabled: Boolean(project),
  })
}

export function useSeries() {
  return useQuery({
    queryKey: seriesKey,
    queryFn: async () => (await call(commands.listSeries)) as Series[],
  })
}

export async function refreshContext() {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: contextKey }),
    queryClient.invalidateQueries({ queryKey: seriesKey }),
    invalidateLineLabels(),
  ])
}

export async function jumpToFlag(layer: EntityId, flag: Flag, pages: { id: EntityId }[]) {
  const page = pages[flag.page - 1]
  if (!page) return
  const selection = await call(commands.selectPage, page.id)
  queryClient.setQueryData(projectKey, selection.project)
  queryClient.setQueryData(pageKey, selection.page)
  useKoharuStore.getState().selectPages([page.id])
  useKoharuStore.getState().selectLayers([layer])
  await refresh(projectKey, pageKey)
}
