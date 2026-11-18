import { pageKey, projectKey, queryClient, type ProjectView } from '@/lib/queries'
import { receivePipeline } from '@/lib/store'
import type { PageFailure, PipelineEvent, Stage } from '@koharu/ipc/protocol'

/** Every stage, in the order a page's stages run. */
export const every: Stage[] = ['detect', 'recognize', 'inpaint', 'translate']

const size = { width: 800, height: 1200 }

/** The project `name` open with `pages`, in order, the canvas showing the first. */
export function openChapter(name: string, pages: string[] = ['p1', 'p2', 'p3', 'p4']): void {
  const project: ProjectView = {
    name,
    page: pages[0] ?? null,
    pages: pages.map((id) => ({ id, size, background: null })),
    history: { steps: [], current: 0 },
    revisions: {},
  }
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pageKey, { id: pages[0], size, background: null, layers: [] })
}

/** Tells `events`, one after another, as the desktop tells them while they happen. */
export function tell(...events: PipelineEvent[]): void {
  for (const event of events) receivePipeline(event)
}

/** Run `run` queued with `stages`, over pages unless `pages` is false. */
export function queued(run: number, stages: Stage[] = every, pages = true): PipelineEvent {
  return { kind: 'queued', run, steps: 4 * stages.length, stages, pages }
}

/** A page given up on in run `run`. */
export function pageFailed(run: number, failure: PageFailure): PipelineEvent {
  return { kind: 'page_failed', run, ...failure }
}

/** Run `run` done with `failures` its problems, or done when there are none. */
export function done(run: number, failures: PageFailure[] = []): PipelineEvent {
  return {
    kind: 'ended',
    run,
    outcome: failures.length ? { kind: 'done_with_problems', failures } : { kind: 'done' },
  }
}
