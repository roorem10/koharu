'use client'

import { useQuery } from '@tanstack/react-query'
import { create } from 'zustand'

import { call } from '@/lib/backend'
import { pageKey, projectKey, queryClient, refresh, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type Bounds,
  type PageCheckFinding,
  type PageCheckPage,
  type ProjectInfo,
} from '@koharu/bridge/protocol'

import { navigateFinding } from './data'

export const pageCheckKey = ['page-check-results'] as const

export function usePageCheckResults() {
  const project = useProject().data
  const jobs = useKoharuStore((state) => state.jobs)
  const progress = Object.values(jobs)
    .filter((job) => job.model === 'page_check')
    .map((job) => `${job.id}:${job.state}:${job.completed}`)
    .join(',')
  return useQuery({
    queryKey: [...pageCheckKey, project?.name, project?.revision, progress],
    queryFn: () => call(commands.getPageCheckResults),
    enabled: Boolean(project),
    placeholderData: (previous, previousQuery) =>
      previousQuery?.queryKey[1] === project?.name ? previous : undefined,
  })
}

// Rough review areas belong to the browser, never to the scene's editable layers.
export const usePageCheckFocus = create<{
  focus: { project: string; page: string; revision: number; area: Bounds } | null
}>()(() => ({ focus: null }))

export async function navigatePageCheck(page: PageCheckPage, finding: PageCheckFinding) {
  const project = queryClient.getQueryData<ProjectInfo | null>(projectKey)
  usePageCheckFocus.setState({ focus: null })
  await navigateFinding({ page: page.page, layer: finding.matched_layer })
  const current = queryClient.getQueryData<ProjectInfo | null>(projectKey)
  if (!current || current.name !== project?.name || current.active_page !== page.page) return
  usePageCheckFocus.setState({
    focus: {
      project: current.name,
      page: page.page,
      revision: current.revision,
      area: finding.area,
    },
  })
}

export async function applyPageCheckFix(page: PageCheckPage, finding: PageCheckFinding) {
  await call(commands.applyPageCheckFix, page.page, finding.id)
  usePageCheckFocus.setState({ focus: null })
  await refresh(projectKey, pageKey, pageCheckKey)
}
