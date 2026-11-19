'use client'

import { useEffect } from 'react'
import { create } from 'zustand'

import i18n from '@/lib/i18n'
import { commands, events, type BooharuJob } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

/** Booharu's jobs going, by id, as the desktop last told them: the reads of a chapter, which tell
 * how they go as `booharu_job`. Explain this line's jobs tell nothing: their command answers. Kept
 * apart from Koharu's runs, which the store keeps by run, as no run works on a read. */
export const useBooharuJobs = create<{ jobs: Record<string, BooharuJob> }>()(() => ({ jobs: {} }))

/** `job` as the desktop told it: kept while it runs, and gone once it ends, whichever way. A read a
 * Run press started that failed or was skipped says why, as the press translates without the
 * chapter's brief; a read by hand answers through its command instead. */
export function receiveBooharuJob(job: BooharuJob): void {
  useBooharuJobs.setState((state) => {
    const jobs = { ...state.jobs }
    if (job.state === 'running') jobs[job.id] = job
    else delete jobs[job.id]
    return { jobs }
  })
  if (job.group !== null && (job.state === 'failed' || job.state === 'skipped')) {
    toast.add({
      type: 'warning',
      title: i18n.t(
        job.state === 'failed' ? 'booharu.runs.read.failed' : 'booharu.runs.read.skipped',
      ),
      description: job.message ?? undefined,
    })
  }
}

/** Starts the jobs from `going`, the reads the desktop answered as going when the interface loaded,
 * leaving out each one `told` names: a job told since listening started, whose event is newer than
 * the answer, which may name a read that has ended since. */
export function seedBooharuJobs(going: readonly BooharuJob[], told: ReadonlySet<number>): void {
  const fresh = going.filter((job) => job.state === 'running' && !told.has(job.id))
  if (!fresh.length) return
  useBooharuJobs.setState((state) => ({
    jobs: { ...state.jobs, ...Object.fromEntries(fresh.map((job) => [job.id, job])) },
  }))
}

/** Follows Booharu's jobs as the desktop tells them, starting from the reads going as the interface
 * loads, until it unmounts. Listening starts first, so nothing told while the reads load is
 * missed. */
export function useBooharuJobEvents(): void {
  useEffect(() => {
    let active = true
    const told = new Set<number>()
    const listening = events.booharuJob((job) => {
      if (!active) return
      told.add(job.id)
      receiveBooharuJob(job)
    })
    void listening
      .then(() => commands.booharuGetJobs())
      .then((going) => {
        if (active) seedBooharuJobs(going, told)
      })
      .catch(() => undefined)
    return () => {
      active = false
      void listening.then((stop) => stop()).catch(() => undefined)
    }
  }, [])
}

/** How many of Booharu's jobs are going, each of which the Activity center lists. */
export function useBooharuJobCount(): number {
  return useBooharuJobs((state) => Object.keys(state.jobs).length)
}

/** Whether a read of the chapter is going, by hand, by the agent, or as a Run press's step: the
 * Run button shows Stop meanwhile, which stops it as it stops a run. */
export function useReadingChapter(): boolean {
  return useBooharuJobs((state) =>
    Object.values(state.jobs).some((job) => job.kind === 'read_chapter'),
  )
}
