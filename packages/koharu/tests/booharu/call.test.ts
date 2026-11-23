import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { call } from '@/lib/booharu/call'
import i18n from '@/lib/i18n'
import { queryClient } from '@/lib/queries'
import { toast } from '@koharu/ui/components/toast'

describe("Booharu's command helper", () => {
  it('toasts a failure once and rejects with an Error carrying its message', async () => {
    const shown = vi.spyOn(toast, 'add')
    const failure = new Error('The layer is locked.')
    const command = vi.fn(async (_layer: string): Promise<null> => {
      throw failure
    })

    await expect(call(command, 'layer-1')).rejects.toBe(failure)

    expect(command).toHaveBeenCalledWith('layer-1')
    expect(shown).toHaveBeenCalledTimes(1)
    expect(shown).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'error', description: 'The layer is locked.' }),
    )
  })

  it('rejects with an Error holding the message the desktop rejects with', async () => {
    const shown = vi.spyOn(toast, 'add')

    const error = await call(() => Promise.reject('No project is open.')).catch(
      (failure: unknown) => failure,
    )

    expect(error).toBeInstanceOf(Error)
    expect((error as Error).message).toBe('No project is open.')
    expect(shown).toHaveBeenCalledTimes(1)
    expect(shown).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'error', description: 'No project is open.' }),
    )
  })

  it('rejects anything else as an unknown error', async () => {
    const shown = vi.spyOn(toast, 'add')

    const error = await call(() => Promise.reject({ code: 7 })).catch((failure: unknown) => failure)

    expect(error).toBeInstanceOf(Error)
    expect((error as Error).message).toBe(i18n.t('errors.unknown'))
    expect(shown).toHaveBeenCalledTimes(1)
  })

  it('answers what the command answers and toasts nothing', async () => {
    const shown = vi.spyOn(toast, 'add')
    const command = vi.fn(async (left: number, right: number) => left + right)

    await expect(call(command, 2, 3)).resolves.toBe(5)

    expect(command).toHaveBeenCalledWith(2, 3)
    expect(shown).not.toHaveBeenCalled()
  })

  describe('once a command settles', () => {
    const mutations = queryClient.getMutationCache()

    beforeEach(() => {
      vi.useFakeTimers()
    })
    afterEach(() => {
      vi.useRealTimers()
    })

    it('lets its mutation go, so the cache drops it and its answer on the next timer turn', async () => {
      const shown = vi.spyOn(toast, 'add')
      const history = { entries: ['the whole agent file, read again after every turn'] }

      await expect(call(async () => history)).resolves.toBe(history)
      // Its gcTime is 0, so the cache drops it on the timers' next turn, not five minutes on.
      await vi.advanceTimersByTimeAsync(0)

      expect(mutations.getAll()).toEqual([])
      expect(shown).not.toHaveBeenCalled()
    })

    it('lets a failed mutation go the same way, its failure toasted once', async () => {
      const shown = vi.spyOn(toast, 'add')

      await expect(call(() => Promise.reject('No project is open.'))).rejects.toThrow(
        'No project is open.',
      )
      await vi.advanceTimersByTimeAsync(0)

      expect(mutations.getAll()).toEqual([])
      expect(shown).toHaveBeenCalledTimes(1)
    })
  })
})
