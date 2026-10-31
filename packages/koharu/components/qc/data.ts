'use client'

import { useQuery } from '@tanstack/react-query'
import { Channel } from '@tauri-apps/api/core'
import { useEffect, useRef, useState } from 'react'

import { refreshContext, useChapterContext } from '@/components/context/data'
import { historyChangedEvent } from '@/components/context/events'
import { call } from '@/lib/backend'
import { pageKey, preparedPageKey, projectKey, queryClient, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { showCanvasPage } from '@koharu/bridge/canvas'
import {
  commands,
  type CanvasPagePreparation,
  type Page,
  type ProjectInfo,
  type QcFinding,
  type QcProgress,
} from '@koharu/bridge/protocol'

export const qcSettingsKey = ['qc-settings'] as const
export const qcKey = (project: string | undefined) => ['qc-results', project] as const

export function useQcResults() {
  const project = useProject().data
  return useQuery({
    queryKey: qcKey(project?.name),
    queryFn: () => call(commands.getQcResults),
    enabled: Boolean(project),
  })
}

export function useQcSettings() {
  return useQuery({ queryKey: qcSettingsKey, queryFn: () => call(commands.getQcSettings) })
}

export function useAutomaticQc() {
  const project = useProject().data
  const jobs = useKoharuStore((state) => state.jobs)
  const styles = useKoharuStore((state) => state.textStyles)
  const chapter = useChapterContext().data
  const settings = useQcSettings().data
  const name = project?.name
  const minimum = settings?.min_font_size
  const [agentCompleted, setAgentCompleted] = useState(0)
  const queue = useRef<Promise<unknown>>(Promise.resolve())
  const completed = Object.values(jobs)
    .filter((job) => job.state !== 'running')
    .map((job) => `${job.id}:${job.state}`)
    .join(',')
  const context = JSON.stringify([chapter?.context.lines, chapter?.series?.glossary])
  useEffect(() => {
    const ended = () => {
      setAgentCompleted((value) => value + 1)
      void refreshContext()
    }
    window.addEventListener(historyChangedEvent, ended)
    return () => window.removeEventListener(historyChangedEvent, ended)
  }, [])
  useEffect(() => {
    if (!name || minimum === undefined) return
    let cancelled = false
    let updateSequence = 0
    const update = async () => {
      const sequence = ++updateSequence
      const report = await commands.getQcResults()
      if (
        !cancelled &&
        sequence === updateSequence &&
        queryClient.getQueryData<ProjectInfo | null>(projectKey)?.name === name
      )
        queryClient.setQueryData(qcKey(name), report)
    }
    const timer = window.setTimeout(() => {
      queue.current = queue.current
        .catch(() => undefined)
        .then(async () => {
          if (cancelled) return
          const channel = new Channel<QcProgress>()
          channel.onmessage = () => {
            if (!cancelled) void update().catch(() => undefined)
          }
          try {
            // A background check may become obsolete while the user continues editing.
            const report = await commands.runQc({ scope: 'project' }, channel, true)
            updateSequence += 1
            if (
              !cancelled &&
              queryClient.getQueryData<ProjectInfo | null>(projectKey)?.name === name
            )
              queryClient.setQueryData(qcKey(name), report)
          } catch {
            // The next debounced revision replaces an obsolete check.
          }
        })
    }, 700)
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [
    name,
    project?.revision,
    project?.active_page,
    completed,
    agentCompleted,
    context,
    minimum,
    styles,
  ])
}

export async function markFinding(finding: QcFinding, hidden: boolean) {
  const project = queryClient.getQueryData<ProjectInfo | null>(projectKey)
  if (finding.flag)
    await call(commands.markQcFinding, finding.layer, finding.check, finding.flag, !hidden)
  else await call(commands.markQcOk, finding.layer, finding.check, !hidden)
  await queryClient.invalidateQueries({ queryKey: qcKey(project?.name) })
  await refreshContext()
}

export async function navigateFinding(finding: QcFinding) {
  const previousProject = queryClient.getQueryData<ProjectInfo | null>(projectKey)
  const previousPage = queryClient.getQueryData<Page | null>(pageKey)
  const prepared = queryClient.getQueryData<CanvasPagePreparation>(preparedPageKey(finding.page))
  const activated = showCanvasPage(finding.page, previousProject?.revision ?? null)
  if (activated && previousProject && prepared?.revision === previousProject.revision) {
    queryClient.setQueryData(projectKey, { ...previousProject, active_page: finding.page })
    queryClient.setQueryData(pageKey, prepared.page)
  }
  useKoharuStore.getState().selectPages([finding.page])
  useKoharuStore.getState().selectLayers([])
  try {
    const selection = await call(commands.selectPage, finding.page)
    if (queryClient.getQueryData<ProjectInfo | null>(projectKey)?.name !== previousProject?.name)
      return
    queryClient.setQueryData(projectKey, selection.project)
    queryClient.setQueryData(pageKey, selection.page)
    useKoharuStore.getState().selectLayers([finding.layer])
  } catch {
    if (
      queryClient.getQueryData<ProjectInfo | null>(projectKey)?.name === previousProject?.name &&
      queryClient.getQueryData<ProjectInfo | null>(projectKey)?.active_page === finding.page
    ) {
      queryClient.setQueryData(projectKey, previousProject)
      queryClient.setQueryData(pageKey, previousPage)
      useKoharuStore.getState().selectPages(previousPage ? [previousPage.id] : [])
    }
  }
}
