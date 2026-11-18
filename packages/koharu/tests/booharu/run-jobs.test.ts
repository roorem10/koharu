import { describe, expect, it } from 'vitest'

import { retryRuns } from '@/lib/booharu/jobs'
import { receiveRuns, useKoharuStore } from '@/lib/store'

import { done, every, openChapter, pageFailed, queued, tell } from './jobs'

const job = (run: number) => useKoharuStore.getState().jobs[String(run)]

const timedOut = "The translation model didn't answer within 10 minutes."

describe('Booharu’s record of a run', () => {
  it('keeps the stages, the target and the project a run was queued with', () => {
    // A run told again as the editor loads, before it reads a project, shows in any.
    receiveRuns([queued(1)])
    expect(job(1)?.booharu).toMatchObject({ stages: every, pages: true, project: null })

    openChapter('Chapter 1')
    tell(queued(2), queued(3, ['translate'], false))
    expect(job(2)).toMatchObject({
      state: 'running',
      total: 16,
      booharu: { stages: every, pages: true, project: 'Chapter 1', retrying: [], failures: [] },
    })
    expect(job(3)?.booharu).toMatchObject({ stages: ['translate'], pages: false })
  })

  it('shows a page waiting for its next try until it starts again or is given up on', () => {
    openChapter('Chapter 1')
    tell(
      queued(1),
      { kind: 'started', run: 1, page: 'p1', stage: 'translate' },
      { kind: 'retrying', run: 1, page: 'p1', stage: 'translate', attempt: 2, attempts: 3 },
      { kind: 'retrying', run: 1, page: 'p2', stage: 'recognize', attempt: 2, attempts: 3 },
    )
    expect(job(1)?.booharu?.retrying).toEqual([
      { page: 'p1', stage: 'translate', attempt: 2, attempts: 3 },
      { page: 'p2', stage: 'recognize', attempt: 2, attempts: 3 },
    ])

    // Another page starting leaves both waiting; the page starting again is no longer waiting.
    tell({ kind: 'started', run: 1, page: 'p3', stage: 'detect' })
    expect(job(1)?.booharu?.retrying).toHaveLength(2)
    tell({ kind: 'started', run: 1, page: 'p1', stage: 'translate' })
    expect(job(1)?.booharu?.retrying).toEqual([
      { page: 'p2', stage: 'recognize', attempt: 2, attempts: 3 },
    ])

    tell(pageFailed(1, { page: 'p2', stage: 'recognize', reason: 'the model failed' }))
    expect(job(1)?.booharu).toMatchObject({
      retrying: [],
      failures: [{ page: 'p2', stage: 'recognize', reason: 'the model failed' }],
    })
    expect(job(1)?.state).toBe('running')
  })

  it('keeps a page stage waiting while another stage of its page starts or finishes', () => {
    openChapter('Chapter 1')
    const reading = { page: 'p1', stage: 'recognize' as const, attempt: 2, attempts: 3 }
    tell(
      queued(1),
      { kind: 'retrying', run: 1, ...reading },
      // Its clean-up needs only detection, so it goes on while its reading waits.
      { kind: 'started', run: 1, page: 'p1', stage: 'inpaint' },
      { kind: 'finished', run: 1, page: 'p1', stage: 'inpaint' },
    )
    expect(job(1)?.booharu?.retrying).toEqual([reading])

    tell({ kind: 'started', run: 1, page: 'p1', stage: 'recognize' })
    expect(job(1)?.booharu?.retrying).toEqual([])
  })

  it('ends done with problems, listing the pages it gave up on', () => {
    openChapter('Chapter 1')
    const failures = [
      { page: 'p2', stage: 'recognize' as const, reason: 'the model failed' },
      { page: 'p4', stage: 'translate' as const, reason: timedOut },
    ]
    tell(
      queued(1),
      pageFailed(1, failures[0]),
      { kind: 'retrying', run: 1, page: 'p4', stage: 'translate', attempt: 3, attempts: 3 },
      pageFailed(1, failures[1]),
      done(1, failures),
    )

    expect(job(1)).toMatchObject({
      state: 'done_with_problems',
      error: null,
      booharu: { retrying: [], failures },
    })
  })

  it('adds the page that ended a failed run to its failed pages', () => {
    openChapter('Chapter 1')
    tell(queued(1), pageFailed(1, { page: 'p1', stage: 'translate', reason: timedOut }), {
      kind: 'ended',
      run: 1,
      outcome: { kind: 'failed', page: 'p3', stage: 'inpaint', reason: 'the disk is full' },
    })

    expect(job(1)).toMatchObject({
      state: 'failed',
      page: 'p3',
      error: 'the disk is full',
      booharu: {
        failures: [
          { page: 'p1', stage: 'translate', reason: timedOut },
          { page: 'p3', stage: 'inpaint', reason: 'the disk is full' },
        ],
      },
    })
  })

  it('retries each failed page from its failed stage through the run’s later stages', () => {
    openChapter('Chapter 1')
    tell(
      queued(1, ['detect', 'recognize', 'translate']),
      pageFailed(1, { page: 'p3', stage: 'recognize', reason: 'the model failed' }),
      pageFailed(1, { page: 'p4', stage: 'translate', reason: timedOut }),
      pageFailed(1, { page: 'p1', stage: 'recognize', reason: 'the model failed' }),
      pageFailed(1, { page: 'p9', stage: 'detect', reason: 'the page was removed' }),
      queued(2, ['translate'], false),
      pageFailed(2, { page: 'p1', stage: 'translate', reason: timedOut }),
    )

    // One run per failed stage, in page order; the page the project no longer has is left out.
    expect(retryRuns(job(1)!, ['p1', 'p2', 'p3', 'p4'])).toEqual([
      { stages: ['recognize', 'translate'], pages: ['p1', 'p3'] },
      { stages: ['translate'], pages: ['p4'] },
    ])
    // A run on blocks or an area offers nothing again.
    expect(retryRuns(job(2)!, ['p1'])).toEqual([])
  })

  it('retries a stage the run dropped as it gave a page up, but not one the page finished', () => {
    openChapter('Chapter 1')
    const order = ['p1', 'p2', 'p3', 'p4']
    // Translate + clean up: a hosted translation runs ahead of the clean-up, so page 2's gave up
    // before its clean-up ran, which left the run with it; page 3's clean-up had finished.
    const failures = [
      { page: 'p2', stage: 'translate' as const, reason: timedOut },
      { page: 'p3', stage: 'translate' as const, reason: timedOut },
    ]
    tell(
      queued(1, ['inpaint', 'translate']),
      { kind: 'finished', run: 1, page: 'p3', stage: 'inpaint' },
      ...failures.map((failure) => pageFailed(1, failure)),
      { kind: 'finished', run: 1, page: 'p1', stage: 'inpaint' },
      done(1, failures),
    )
    expect(retryRuns(job(1)!, order)).toEqual([
      { stages: ['inpaint', 'translate'], pages: ['p2'] },
      { stages: ['translate'], pages: ['p3'] },
    ])

    // Every stage: page 1's clean-up failed while its reading waited for another try, and page 4's
    // failed after its reading finished. A page's translation is done again after its clean-up.
    const cleanUps = [
      { page: 'p1', stage: 'inpaint' as const, reason: 'CUDA out of memory' },
      { page: 'p4', stage: 'inpaint' as const, reason: 'CUDA out of memory' },
    ]
    tell(
      queued(2),
      { kind: 'finished', run: 2, page: 'p1', stage: 'detect' },
      { kind: 'finished', run: 2, page: 'p4', stage: 'detect' },
      { kind: 'finished', run: 2, page: 'p4', stage: 'recognize' },
      { kind: 'finished', run: 2, page: 'p4', stage: 'translate' },
      ...cleanUps.map((failure) => pageFailed(2, failure)),
      done(2, cleanUps),
    )
    expect(retryRuns(job(2)!, order)).toEqual([
      { stages: ['recognize', 'inpaint', 'translate'], pages: ['p1'] },
      { stages: ['inpaint', 'translate'], pages: ['p4'] },
    ])
  })
})
