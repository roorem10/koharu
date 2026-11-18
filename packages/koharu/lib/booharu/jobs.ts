'use client'

import { projectKey, queryClient, type ProjectView } from '@/lib/queries'
import type { Job } from '@/lib/store'
import type { Outcome, PageFailure, PageId, PipelineEvent, Stage } from '@koharu/ipc/protocol'

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

/** The open project's name, as the editor last read it; null while none is known. */
function openProject(): string | null {
  return queryClient.getQueryData<ProjectView | null>(projectKey)?.name ?? null
}
