import type { MutationState } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { queryClient, send } from '@/lib/queries'
import { toast } from '@koharu/ui/components/toast'

// Koharu's send() runs a command as a mutation nothing waits on; Booharu's marked lines let the
// mutation go once the command settles, with gcTime 0, as call() does, so the cache drops it on
// the next timer turn (koharu-rs/koharu#1477).
describe("Koharu's send()", () => {
  const mutations = queryClient.getMutationCache()
  /** Resolves, with the state its mutation settled in, once send() lets go of the mutation. */
  const letGo = () =>
    new Promise<MutationState>((resolve) => {
      const stop = mutations.subscribe((event) => {
        if (event.type !== 'observerRemoved') return
        stop()
        resolve(event.mutation.state)
      })
    })

  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('lets a mutation go once its command answers, so the cache drops it and its answer on the next timer turn', async () => {
    const shown = vi.spyOn(toast, 'add')
    const history = { entries: ['the whole agent file, read again after every turn'] }
    const command = vi.fn(async (_page: string) => history)
    const released = letGo()

    send(command, 'page-1')
    const settled = await released

    expect(command).toHaveBeenCalledWith('page-1')
    expect(settled.status).toBe('success')
    expect(settled.data).toBe(history)
    expect(shown).not.toHaveBeenCalled()

    // Its gcTime is 0, so the cache drops it on the timers' next turn, not five minutes on.
    await vi.advanceTimersByTimeAsync(0)

    expect(mutations.getAll()).toEqual([])
  })

  it('lets a failed mutation go the same way, its failure toasted once', async () => {
    const shown = vi.spyOn(toast, 'add')
    const released = letGo()

    send(() => Promise.reject('No project is open.'))
    const settled = await released

    expect(settled.status).toBe('error')
    expect(shown).toHaveBeenCalledTimes(1)
    expect(shown).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'error', description: 'No project is open.' }),
    )

    await vi.advanceTimersByTimeAsync(0)

    expect(mutations.getAll()).toEqual([])
    expect(shown).toHaveBeenCalledTimes(1)
  })
})
