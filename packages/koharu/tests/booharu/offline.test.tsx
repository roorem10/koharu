import { onlineManager, QueryClientProvider } from '@tanstack/react-query'
import { act, renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { call } from '@/lib/booharu/call'
import { booharuPreferencesQuery } from '@/lib/booharu/preferences'
import { queryClient, send, useProject, useSavePreferences } from '@/lib/queries'
import { commands, type Preferences } from '@koharu/ipc/protocol'

import { settingsDesktop } from './settings'

// Every command goes to the desktop's own backend, never to the internet, so each runs at once
// while the computer is offline, as when its Wi-Fi drops; in TanStack's default network mode a
// query or a mutation would wait until the browser said it was online again.

const preferences: Preferences = {
  pipeline: {},
  renderer: { font_families: [] },
  agent: {},
  providers: {},
}

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
}

/** What `promise` settles to, or a failure once a second has passed without it settling. */
function soon<T>(promise: Promise<T>): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined
  const late = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error('still waiting after a second')), 1000)
  })
  return Promise.race([promise, late]).finally(() => clearTimeout(timer))
}

describe('commands while the computer is offline', () => {
  beforeEach(() => {
    // Mounted, as QueryClientProvider mounts it in the app, the client hears the browser's
    // online and offline events.
    queryClient.mount()
    window.dispatchEvent(new Event('offline'))
    expect(onlineManager.isOnline()).toBe(false)
  })
  afterEach(() => {
    onlineManager.setOnline(true)
    queryClient.unmount()
  })

  it("runs a command Koharu's send() sends", async () => {
    const command = vi.fn(async (_page: string) => null)

    send(command, 'page-1')

    await vi.waitFor(() => expect(command).toHaveBeenCalledWith('page-1'))
  })

  it("answers a command Booharu's call() runs", async () => {
    const add = vi.fn(async (left: number, right: number) => left + right)

    await expect(soon(call(add, 2, 3))).resolves.toBe(5)
  })

  it('reads a query fetched through the client', async () => {
    const { file } = settingsDesktop()

    await expect(soon(queryClient.fetchQuery(booharuPreferencesQuery))).resolves.toEqual(
      file.preferences,
    )
  })

  it('reads the open project the editor shows', async () => {
    vi.spyOn(commands, 'getOpenProject').mockResolvedValue('Chapter 1')
    vi.spyOn(commands, 'getShownPage').mockResolvedValue(null)
    vi.spyOn(commands, 'query').mockImplementation(async (query) =>
      query.query === 'get_project'
        ? { id: 'project', version: '1', pages: [] }
        : { steps: [], current: 0 },
    )

    const { result } = renderHook(() => useProject(), { wrapper })

    await waitFor(() =>
      expect(result.current.data).toMatchObject({ name: 'Chapter 1', page: null, pages: [] }),
    )
  })

  it("saves Koharu's settings through their keyed mutation", async () => {
    const save = vi.spyOn(commands, 'savePreferences').mockImplementation(async (saved) => saved)
    const { result } = renderHook(() => useSavePreferences(), { wrapper })

    act(() => result.current.mutate(preferences))

    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(save).toHaveBeenCalledWith(preferences)
  })
})
