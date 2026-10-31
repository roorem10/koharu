'use client'

import { create } from 'zustand'

import { call } from '@/lib/backend'
import { projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type CleanupRect,
  type CleanupStroke,
  type EntityId,
  type JobId,
  type ProjectInfo,
} from '@koharu/bridge/protocol'

export type CleanupArea = { id: number } & (
  | { kind: 'stroke'; stroke: CleanupStroke }
  | { kind: 'rect'; rect: CleanupRect }
)
interface PendingClean {
  areas: CleanupArea[]
  job: JobId | null
}
interface CleanupQueue {
  project: string | null
  generation: number
  nextId: number
  pages: Record<EntityId, CleanupArea[]>
  pending: Record<EntityId, PendingClean>
  drawing: boolean
}

export const useCleanupQueue = create<CleanupQueue>()(() => ({
  project: null,
  generation: 0,
  nextId: 1,
  pages: {},
  pending: {},
  drawing: false,
}))

export function syncCleanupProject(project: string | null) {
  const state = useCleanupQueue.getState()
  if (state.project !== project)
    useCleanupQueue.setState({
      project,
      generation: state.generation + 1,
      nextId: 1,
      pages: {},
      pending: {},
      drawing: false,
    })
}

export function addCleanupArea(
  page: EntityId,
  area: { kind: 'stroke'; stroke: CleanupStroke } | { kind: 'rect'; rect: CleanupRect },
) {
  if (useCleanupQueue.getState().project === null) return
  useCleanupQueue.setState((state) => ({
    nextId: state.nextId + 1,
    pages: {
      ...state.pages,
      [page]: [...(state.pages[page] ?? []), { ...area, id: state.nextId }],
    },
  }))
}

export function clearCleanupAreas(page: EntityId) {
  useCleanupQueue.setState((state) => ({ pages: { ...state.pages, [page]: [] } }))
}

export function processingBusy() {
  return Object.values(useKoharuStore.getState().jobs).some((job) => job.state === 'running')
}

export async function cleanQueuedAreas(page: EntityId) {
  const state = useCleanupQueue.getState()
  const areas = state.pages[page] ?? []
  if (!areas.length || state.pending[page] || state.drawing || processingBusy()) return
  const generation = state.generation
  useCleanupQueue.setState({ pending: { ...state.pending, [page]: { areas, job: null } } })
  try {
    const job = await call(
      commands.cleanAreas,
      page,
      areas.flatMap((area) => (area.kind === 'stroke' ? [area.stroke] : [])),
      areas.flatMap((area) => (area.kind === 'rect' ? [area.rect] : [])),
    )
    const latest = useCleanupQueue.getState()
    if (latest.generation !== generation) return
    const submitted = new Set(areas.map((area) => area.id))
    useCleanupQueue.setState({
      pages: {
        ...latest.pages,
        [page]: (latest.pages[page] ?? []).filter((area) => !submitted.has(area.id)),
      },
      pending: { ...latest.pending, [page]: { areas, job } },
    })
    settleCleanupJobs()
  } catch (error) {
    if (useCleanupQueue.getState().generation === generation) {
      const pending = { ...useCleanupQueue.getState().pending }
      delete pending[page]
      useCleanupQueue.setState({ pending })
    }
    throw error
  }
}

export function settleCleanupJobs() {
  const state = useCleanupQueue.getState()
  const jobs = useKoharuStore.getState().jobs
  const pending = { ...state.pending }
  const pages = { ...state.pages }
  let changed = false
  for (const [page, submitted] of Object.entries(pending)) {
    const job = submitted.job ? jobs[submitted.job] : null
    if (!job || job.state === 'running') continue
    if (job.state !== 'finished') pages[page] = [...submitted.areas, ...(pages[page] ?? [])]
    delete pending[page]
    changed = true
  }
  if (changed) useCleanupQueue.setState({ pending, pages })
}

// Settings unmounts the editor. Queue lifetime follows the project, not that screen.
queryClient.getQueryCache().subscribe((event) => {
  if (event.query.queryKey.length !== 1 || event.query.queryKey[0] !== projectKey[0]) return
  if (event.type === 'removed') syncCleanupProject(null)
  else if (event.type === 'updated' && event.query.state.data !== undefined) {
    syncCleanupProject((event.query.state.data as ProjectInfo | null)?.name ?? null)
  }
})
useKoharuStore.subscribe((state, previous) => {
  if (state.jobs !== previous.jobs) settleCleanupJobs()
})
