'use client'

import { useEffect } from 'react'

import { playFinishSound } from '@/components/booharu/runs/sound'
import { finishSoundOn } from '@/lib/booharu/runs'
import type { Job } from '@/lib/store'
import { events, type BooharuRunGroup, type Outcome, type RunId } from '@koharu/ipc/protocol'

/** Each run a carrying-on group queued, by run, as `booharu_run_group` tells it, until it ends. */
const groups = new Map<RunId, BooharuRunGroup>()
/** The groups going on, by number, whose runs so far had problems. */
const troubled = new Set<number>()

/** Notes a run a carrying-on group queued, so the group chimes once, as it ends. */
export function receiveRunGroup(group: BooharuRunGroup): void {
  groups.set(group.run, group)
}

/** Notes each run a carrying-on group queues, whoever started the group, as the desktop tells it,
 * until the interface unmounts. */
export function useBooharuRunGroups(): void {
  useEffect(() => {
    let active = true
    const listening = events.booharuRunGroup((group) => {
      if (active) receiveRunGroup(group)
    })
    return () => {
      active = false
      void listening.then((stop) => stop()).catch(() => undefined)
    }
  }, [])
}

/** How an ending sounds: `done`, or `problems`, lower, when a page failed. */
export type Tone = 'done' | 'problems'

/** What `job` ending with `outcome` chimes, whoever started it. A run over pages chimes as it ends:
 * done when it was done, and with problems when it was done with problems or failed; one that
 * stopped stays silent, and so does a run on blocks or an area. A run of a carrying-on group chimes
 * for the group instead, once, as the group ends: with its last run, or the run that failed and
 * ended it, with problems when any of its runs had them, and never once a run stopped it. A group
 * that ends otherwise, as when the pages of its later groups were removed while it went, isn't
 * told to end, and ends silent. */
export function ending(job: Job, outcome: Outcome): Tone | null {
  let problems = outcome.kind === 'done_with_problems' || outcome.kind === 'failed'
  const group = groups.get(job.id)
  if (group) {
    groups.delete(job.id)
    if (problems) troubled.add(group.group)
    // After a run done, with problems or not, the group's next run follows, unless it was its last.
    const over =
      outcome.kind === 'failed' || outcome.kind === 'stopped' || group.index === group.count - 1
    if (!over) return null
    problems = troubled.delete(group.group)
  }
  if (outcome.kind === 'stopped' || !job.booharu?.pages) return null
  return problems ? 'problems' : 'done'
}

/** Chimes for `job`, told live as it ended with `outcome`, when its ending chimes. */
export function chime(job: Job, outcome: Outcome): void {
  const tone = ending(job, outcome)
  if (tone) finished(tone)
}

/** Plays `tone`, a lone run's or a group's ending, while the finish sound is on.
 *
 * Stage 16's batches chime once per batch: a batch going takes the endings of its runs and groups
 * here, in place of their chimes, and plays one tone as it ends, `problems` when any had them. */
function finished(tone: Tone): void {
  void finishSoundOn().then((on) => {
    if (on) playFinishSound(tone === 'problems')
  })
}
