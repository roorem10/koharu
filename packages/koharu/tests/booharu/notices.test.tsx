import { renderHook, waitFor } from '@testing-library/react'
import { StrictMode } from 'react'
import { describe, expect, it, vi } from 'vitest'

import { useBooharuNotices } from '@/lib/booharu/notices'
import { events, type BooharuNotice } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

describe('Booharu notices', () => {
  it('shows each notice once, in the interface language, until it unmounts', async () => {
    // Every handler the hook listened with, as the desktop would tell them.
    const handlers: ((notice: BooharuNotice) => void)[] = []
    const stop = vi.fn()
    vi.spyOn(events, 'booharuNotice').mockImplementation(async (handler) => {
      handlers.push(handler)
      return stop
    })
    const tell = (notice: BooharuNotice) => handlers.forEach((handler) => handler(notice))
    const shown = vi.spyOn(toast, 'add')
    // Strict Mode runs the effect twice; the first listener stops and tells nothing.
    const hook = renderHook(() => useBooharuNotices(), { wrapper: StrictMode })
    expect(handlers).toHaveLength(2)

    // A K tool box's block, which stays either way.
    tell({ kind: 'nothing_to_read' })
    tell({ kind: 'failed', message: 'the page has no scan' })
    expect(shown.mock.calls.map(([options]) => [options.type, options.title])).toEqual([
      ['info', 'Nothing to read in that box. Its text block was kept, for you to type into.'],
      ['error', "The text block was added but couldn't be read: the page has no scan"],
    ])

    hook.unmount()
    await waitFor(() => expect(stop).toHaveBeenCalledTimes(handlers.length))
    tell({ kind: 'nothing_to_read' })
    expect(shown).toHaveBeenCalledTimes(2)
  })
})
