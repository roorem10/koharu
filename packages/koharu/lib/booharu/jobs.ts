'use client'

import { prepareFinishSound } from '@/components/booharu/runs/sound'
import { call } from '@/lib/backend'
import { chime } from '@/lib/booharu/chime'
import { projectKey, queryClient, useProject, type ProjectView } from '@/lib/queries'
import { pipelineStages, useKoharuStore, type Job } from '@/lib/store'
import {
  commands,
  type Outcome,
  type PageFailure,
  type PageId,
  type PipelineEvent,
  type Stage,
} from '@koharu/ipc/protocol'

// lib/store.ts imports this module, which reaches the store again through its own imports: safe, as
// neither module reads the other's exports while it loads, only in calls made after.

/** A page waiting for its next try: `attempt` is the try it waits for, its first counting as 1,
 * of `attempts`. */
export type Retry = { page: PageId; stage: Stage; attempt: number; attempts: number }

/** What Booharu keeps of a run beside Koharu's counts, from the pipeline's events. */
export interface RunRecord {
  /** Its stages, in stage order. */
  stages: Stage[]
  /** Whether it works on pages, the project's or some, rather than on blocks or an area: only
   * such a run chimes, and offers its failed pages again. */
  pages: boolean
  /** The project open when it was queued, by name, the only one whose Activity center lists it;
   * null when none was known, as for a run told again as the editor loads, which shows in any. */
  project: string | null
  /** Its pages waiting for their next try, the latest last. */
  retrying: Retry[]
  /** The pages it gave up on, and for a run that failed, the page that ended it, in the order
   * they failed. */
  failures: PageFailure[]
}

/** The record of a run whose Queued the interface missed: its stages and target unknown, so it
 * neither chimes nor offers its pages again, and it shows in any project. */
const missed: RunRecord = { stages: [], pages: false, project: null, retrying: [], failures: [] }

/** `job` with Booharu's record of the run kept through `event`, after Koharu's fold of it:
 * Queued's stages and whether it works on pages, with the project open; each page waiting for its
 * next try, until the page starts or finishes a stage, or is given up on; and the failed pages,
 * which a run done with problems lists as it ends, and a run that failed adds its page to. */
export function foldRun(job: Job, event: PipelineEvent): Job {
  if (event.kind === 'queued') {
    const { stages, pages } = event
    return {
      ...job,
      booharu: { stages, pages, project: openProject(), retrying: [], failures: [] },
    }
  }
  const record = job.booharu ?? missed
  const change = (update: Partial<RunRecord>): Job => ({
    ...job,
    booharu: { ...record, ...update },
  })
  const waiting = (page: PageId) => record.retrying.filter((retry) => retry.page !== page)
  switch (event.kind) {
    case 'retrying': {
      const { page, stage, attempt, attempts } = event
      return change({ retrying: [...waiting(page), { page, stage, attempt, attempts }] })
    }
    case 'started':
    case 'finished':
      return record.retrying.some((retry) => retry.page === event.page)
        ? change({ retrying: waiting(event.page) })
        : job
    case 'page_failed': {
      const { page, stage, reason } = event
      return change({
        retrying: waiting(page),
        failures: [...record.failures, { page, stage, reason }],
      })
    }
    case 'ended':
      return change({ retrying: [], failures: failuresAtEnd(record.failures, event.outcome) })
  }
}

/** The failed pages of a run that ended with `outcome`, `failures` so far: those a run done with
 * problems lists, or with the page that ended a run that failed. */
function failuresAtEnd(failures: PageFailure[], outcome: Outcome): PageFailure[] {
  switch (outcome.kind) {
    case 'done_with_problems':
      return outcome.failures
    case 'failed': {
      const { page, stage, reason } = outcome
      return failures.some((failure) => failure.page === page)
        ? failures
        : [...failures, { page, stage, reason }]
    }
    default:
      return failures
  }
}

/** What follows `event` as it's told, after its fold, which a run told again as the editor loads
 * doesn't get: a run that ended chimes, as lib/booharu/chime.ts says. */
export function toldLive(event: PipelineEvent): void {
  if (event.kind !== 'ended') return
  const job = useKoharuStore.getState().jobs[String(event.run)]
  if (job) chime(job, event.outcome)
}

/** The open project's name, as the editor last read it; null while none is known. */
function openProject(): string | null {
  return queryClient.getQueryData<ProjectView | null>(projectKey)?.name ?? null
}

/** Whether `job` was queued in `project`, the project open now, or where none was known. */
function inProject(job: Job, project: string | null): boolean {
  const queuedIn = job.booharu?.project ?? null
  return queuedIn === null || queuedIn === project
}

/** The runs the Activity center lists, in run order: Koharu's, running or failed, and a run done
 * with problems, each only while the project it was queued in is open. */
export function useVisibleJobs(jobs: Record<string, Job>): Job[] {
  const project = useProject().data?.name ?? null
  return Object.values(jobs).filter(
    (job) =>
      (job.state === 'running' || job.state === 'failed' || job.state === 'done_with_problems') &&
      inProject(job, project),
  )
}

/** Whether the Activity center lists `job`'s failed pages in place of its row: a run done with
 * problems, or a run over pages that failed. A run on blocks or an area that failed keeps Koharu's
 * row. */
export function listsFailedPages(job: Job): boolean {
  return (
    job.state === 'done_with_problems' || (job.state === 'failed' && job.booharu?.pages === true)
  )
}

/** A run Retry failed pages queues: `stages` on `pages`. */
export type RetryRun = { stages: Stage[]; pages: PageId[] }

/** The runs Retry failed pages queues for `job`: one for each stage a page failed at, in stage
 * order, on the pages that failed at it, of `order`, the project's pages in order, so a page the
 * project no longer has is left out; each from that stage on through `job`'s later stages, so
 * those it finished aren't done again. None for a run on blocks or an area. */
export function retryRuns(job: Job, order: readonly PageId[]): RetryRun[] {
  const record = job.booharu
  if (!record?.pages) return []
  return pipelineStages.flatMap((stage, at) => {
    const atStage = new Set(
      record.failures.filter((failure) => failure.stage === stage).map((failure) => failure.page),
    )
    const pages = order.filter((page) => atStage.has(page))
    if (!pages.length) return []
    const stages = pipelineStages.filter(
      (later, from) => from >= at && (later === stage || record.stages.includes(later)),
    )
    return [{ stages, pages }]
  })
}

/** The open project's pages, in order. */
function pageOrder(): PageId[] {
  const project = queryClient.getQueryData<ProjectView | null>(projectKey)
  return project?.pages.map((page) => page.id) ?? []
}

/** Retry failed pages: queues `job`'s retry runs, one after another, each an ordinary run, and once
 * they're queued, takes the job's failed pages off the Activity center. The click readies the
 * finish chime. A run that can't be queued shows its error as the Run button's do, and leaves
 * them. */
export async function retryFailedPages(job: Job): Promise<void> {
  prepareFinishSound()
  for (const { stages, pages } of retryRuns(job, pageOrder())) {
    await call(commands.runPipeline, { pages }, stages)
  }
  useKoharuStore.getState().dismissJob(String(job.id))
}

/** The last run in the open project, while it has failed pages to queue again: the Run menu offers
 * Retry failed pages for it then. */
export function useRetryable(): Job | null {
  const jobs = useKoharuStore((state) => state.jobs)
  const project = useProject().data
  const name = project?.name ?? null
  const last = Object.values(jobs)
    .filter((job) => inProject(job, name))
    .at(-1)
  const order = project?.pages.map((page) => page.id) ?? []
  return last && listsFailedPages(last) && retryRuns(last, order).length ? last : null
}

/** Shows a failed page as clicking it in the page rail does: it alone selected among the pages,
 * and, when the canvas shows another, no layer. */
export function showFailedPage(page: PageId): void {
  const { selectPages, selectLayers } = useKoharuStore.getState()
  selectPages([page])
  if (queryClient.getQueryData<ProjectView | null>(projectKey)?.page === page) return
  selectLayers([])
  void call(commands.show, page).catch(() => undefined)
}
