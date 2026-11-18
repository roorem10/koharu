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
  /** For a run over pages, the stages each page finished, so Retry failed pages runs again a stage
   * the run dropped as it gave the page up, and not one it finished. */
  finished: Partial<Record<PageId, Stage[]>>
}

/** The record of a run whose Queued the interface missed: its stages and target unknown, so it
 * neither chimes nor offers its pages again, and it shows in any project. */
const missed: RunRecord = {
  stages: [],
  pages: false,
  project: null,
  retrying: [],
  failures: [],
  finished: {},
}

/** `job` with Booharu's record of the run kept through `event`, after Koharu's fold of it:
 * Queued's stages and whether it works on pages, with the project open; each page stage waiting
 * for its next try, until that stage starts or finishes, or its page is given up on; the stages
 * each page finished; and the failed pages, which a run done with problems lists as it ends, and a
 * run that failed adds its page to. */
export function foldRun(job: Job, event: PipelineEvent): Job {
  if (event.kind === 'queued') {
    const { stages, pages } = event
    return {
      ...job,
      booharu: { stages, pages, project: openProject(), retrying: [], failures: [], finished: {} },
    }
  }
  const record = job.booharu ?? missed
  const change = (update: Partial<RunRecord>): Job => ({
    ...job,
    booharu: { ...record, ...update },
  })
  /** The waits but `page`'s at `stage`, or at any stage when none is named. */
  const waiting = (page: PageId, stage?: Stage) =>
    record.retrying.filter(
      (retry) => retry.page !== page || (stage !== undefined && retry.stage !== stage),
    )
  switch (event.kind) {
    case 'retrying': {
      const { page, stage, attempt, attempts } = event
      return change({ retrying: [...waiting(page, stage), { page, stage, attempt, attempts }] })
    }
    case 'started':
    case 'finished': {
      const { page, stage } = event
      const retrying = waiting(page, stage)
      const finished =
        event.kind === 'finished' && record.pages
          ? { ...record.finished, [page]: [...(record.finished[page] ?? []), stage] }
          : record.finished
      return retrying.length === record.retrying.length && finished === record.finished
        ? job
        : change({ retrying, finished })
    }
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
 * with problems, each only while the project it was queued in is open. A run that stopped isn't
 * listed, whatever pages it gave up on, as Koharu doesn't list it; the Run menu offers them. */
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

/** The stages a page's stage waits for in a run, as Koharu's run graph orders them: detection
 * before every other stage, and recognition before translation. A page that got to a stage had
 * finished these, whatever events the interface heard. */
const waitsFor: Record<Stage, readonly Stage[]> = {
  detect: [],
  recognize: ['detect'],
  inpaint: ['detect'],
  translate: ['detect', 'recognize'],
}

/** The stages Retry failed pages runs for a page of `record`'s run that failed at `stage`, having
 * finished `done`: from `stage` on through the run's later stages, so a later stage it finished
 * is done again on what the retry makes; and an earlier stage of the run it never finished and
 * `stage` doesn't wait for, which the run dropped as it gave the page up, such as its clean-up
 * when its translation failed first. */
function retryStages(record: RunRecord, stage: Stage, done: readonly Stage[]): Stage[] {
  const at = pipelineStages.indexOf(stage)
  return pipelineStages.filter(
    (other, index) =>
      other === stage ||
      (record.stages.includes(other) &&
        (index > at || (!waitsFor[stage].includes(other) && !done.includes(other)))),
  )
}

/** The runs Retry failed pages queues for `job`, for its failed pages in `order`, the project's
 * pages in order, so a page the project no longer has is left out: one for each set of stages
 * `retryStages` gives them, in stage order, on the pages that need it. None for a run on blocks
 * or an area. */
export function retryRuns(job: Job, order: readonly PageId[]): RetryRun[] {
  const record = job.booharu
  if (!record?.pages) return []
  const failedAt = new Map<PageId, Stage>()
  for (const { page, stage } of record.failures) {
    const before = failedAt.get(page)
    if (!before || pipelineStages.indexOf(stage) < pipelineStages.indexOf(before)) {
      failedAt.set(page, stage)
    }
  }
  const runs = new Map<string, RetryRun>()
  for (const page of order) {
    const stage = failedAt.get(page)
    if (!stage) continue
    const stages = retryStages(record, stage, record.finished[page] ?? [])
    const run = runs.get(stages.join())
    if (run) run.pages.push(page)
    else runs.set(stages.join(), { stages, pages: [page] })
  }
  const first = (run: RetryRun) => pipelineStages.indexOf(run.stages[0] ?? 'detect')
  return [...runs.values()].sort((a, b) => first(a) - first(b))
}

/** The open project's pages, in order. */
function pageOrder(): PageId[] {
  const project = queryClient.getQueryData<ProjectView | null>(projectKey)
  return project?.pages.map((page) => page.id) ?? []
}

/** Retry failed pages: queues `job`'s retry runs, one after another, each an ordinary run, taking
 * each run's pages off the job's failed pages once it's queued, and the job off the Activity
 * center once every one is. The click readies the finish chime. A run that can't be queued shows
 * its error as the Run button's do, and leaves the pages not yet queued, which a later click
 * retries alone. */
export async function retryFailedPages(job: Job): Promise<void> {
  prepareFinishSound()
  const id = String(job.id)
  for (const { stages, pages } of retryRuns(job, pageOrder())) {
    await call(commands.runPipeline, { pages }, stages)
    retried(id, pages)
  }
  useKoharuStore.getState().dismissJob(id)
}

/** Takes `pages`, which a retry queued, off job `id`'s failed pages. */
function retried(id: string, pages: readonly PageId[]): void {
  useKoharuStore.setState((state) => {
    const job = state.jobs[id]
    if (!job?.booharu) return {}
    const failures = job.booharu.failures.filter((failure) => !pages.includes(failure.page))
    return { jobs: { ...state.jobs, [id]: { ...job, booharu: { ...job.booharu, failures } } } }
  })
}

/** The last run in the open project, once it ended with failed pages to queue again, whether it
 * was done with problems, failed, or stopped after giving up on pages: the Run menu offers Retry
 * failed pages for it then. */
export function useRetryable(): Job | null {
  const jobs = useKoharuStore((state) => state.jobs)
  const project = useProject().data
  const name = project?.name ?? null
  const last = Object.values(jobs)
    .filter((job) => inProject(job, name))
    .at(-1)
  const order = project?.pages.map((page) => page.id) ?? []
  return last && last.state !== 'running' && retryRuns(last, order).length ? last : null
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
