import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { queryClient, send } from '@/lib/queries'
import { toast } from '@koharu/ui/components/toast'

// Koharu's send() runs a command as a mutation nothing waits on; Booharu's marked line lets the
// mutation go once the command settles, as call() does, so the cache drops it after its gcTime.
describe("Koharu's send()", () => {
  const mutations = queryClient.getMutationCache()
  /** Waits until the cache's mutations have settled as `statuses` says, in order. */
  const settled = (...statuses: string[]) =>
    vi.waitFor(() => expect(mutations.getAll().map(({ state }) => state.status)).toEqual(statuses))
  /** A mutation's gcTime, TanStack's five minutes. */
  const gcTime = 5 * 60 * 1000

  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('lets a mutation go once its command answers, so the cache drops it and its answer', async () => {
    const shown = vi.spyOn(toast, 'add')
    const history = { entries: ['the whole agent file, read again after every turn'] }
    const command = vi.fn(async (_page: string) => history)

    send(command, 'page-1')
    await settled('success')

    expect(command).toHaveBeenCalledWith('page-1')
    expect(mutations.getAll()[0]?.state.data).toBe(history)
    expect(shown).not.toHaveBeenCalled()

    // A mutation nothing observes leaves the cache once its gcTime has passed.
    await vi.advanceTimersByTimeAsync(gcTime)

    expect(mutations.getAll()).toEqual([])
  })

  it('lets a failed mutation go too, its failure toasted once', async () => {
    const shown = vi.spyOn(toast, 'add')

    send(() => Promise.reject('No project is open.'))
    await settled('error')

    expect(shown).toHaveBeenCalledTimes(1)
    expect(shown).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'error', description: 'No project is open.' }),
    )

    await vi.advanceTimersByTimeAsync(gcTime)

    expect(mutations.getAll()).toEqual([])
    expect(shown).toHaveBeenCalledTimes(1)
  })
})
