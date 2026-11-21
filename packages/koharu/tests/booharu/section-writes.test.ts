import { MutationObserver, QueryObserver } from '@tanstack/react-query'
import { describe, expect, it, vi } from 'vitest'

import {
  booharuPreferencesKey,
  queueBooharuWrite,
  refreshPreferences,
  saveBooharuSection,
  writeBooharuSection,
} from '@/lib/booharu/preferences'
import { preferencesKey, queryClient } from '@/lib/queries'
import { commands, type Preferences } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { settingsDesktop } from './settings'

describe("Booharu's settings section writes", () => {
  it('saves a section over what the file holds, and shows what the save left', async () => {
    const { file, saves } = settingsDesktop()
    file.preferences['booharu-qc'] = { min_size: 0.02 }

    const saved = await saveBooharuSection('booharu-runs', (runs) => ({ ...runs, retries: 5 }))

    expect(saves).toHaveLength(1)
    expect(saves[0]?.['booharu-runs'].retries).toBe(5)
    expect(saves[0]?.['booharu-qc']).toEqual({ min_size: 0.02 })
    expect(queryClient.getQueryData(booharuPreferencesKey)).toEqual(saved)
  })

  it('toasts a failed save once, outside a mutation and inside one', async () => {
    const { save } = settingsDesktop()
    save.mockRejectedValue('The settings file is read-only.')
    const shown = vi.spyOn(toast, 'add')

    const error = await saveBooharuSection('booharu-qc', (qc) => qc).catch((e: unknown) => e)
    expect(error).toBeInstanceOf(Error)
    expect((error as Error).message).toBe('The settings file is read-only.')
    expect(shown).toHaveBeenCalledTimes(1)

    shown.mockClear()
    const mutation = new MutationObserver(queryClient, {
      mutationFn: () => writeBooharuSection('booharu-qc', (qc) => qc),
    })
    await expect(mutation.mutate()).rejects.toBe('The settings file is read-only.')
    expect(shown).toHaveBeenCalledTimes(1)
  })

  it('queues a save at once, before a write queued after the call', async () => {
    settingsDesktop()
    const order: string[] = []
    vi.mocked(commands.booharuSavePreferences).mockImplementation(async (preferences) => {
      order.push('save')
      return preferences
    })

    const saved = saveBooharuSection('booharu-qc', (qc) => qc)
    const next = queueBooharuWrite(async () => {
      order.push('next')
    })
    await Promise.all([saved, next])

    expect(order).toEqual(['save', 'next'])
  })
})

describe("reading Koharu's settings again", () => {
  it('waits for a settings save under way, then reads what the file holds', async () => {
    const before = { pipeline: {} } as unknown as Preferences
    const after = { pipeline: { translator: { model: 'openai/gpt' } } } as unknown as Preferences
    const read = vi.spyOn(commands, 'getPreferences').mockResolvedValue(after)
    queryClient.setQueryData(preferencesKey, before)
    // Settings showing them, so a refresh reads them again.
    const shown = new QueryObserver(queryClient, {
      queryKey: preferencesKey,
      queryFn: () => commands.getPreferences(),
    })
    const stop = shown.subscribe(() => undefined)
    let finish = () => {}
    const saving = new MutationObserver(queryClient, {
      mutationKey: preferencesKey,
      mutationFn: () => new Promise<void>((resolve) => (finish = resolve)),
    }).mutate()

    let done = false
    const refreshed = refreshPreferences().then(() => (done = true))
    await new Promise((resolve) => setTimeout(resolve, 20))
    expect(done).toBe(false)
    expect(read).not.toHaveBeenCalled()

    finish()
    await saving
    await refreshed
    expect(read).toHaveBeenCalledTimes(1)
    expect(queryClient.getQueryData(preferencesKey)).toEqual(after)
    stop()
  })
})
