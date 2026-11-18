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

    tell({ kind: 'nothing_to_read' })
    tell({ kind: 'locked' })
    tell({ kind: 'failed', message: 'the page has no scan' })
    expect(shown.mock.calls.map(([options]) => [options.type, options.title])).toEqual([
      ['info', 'Nothing to read in that box.'],
      ['info', 'The layer that box would go in is locked. Unlock it first.'],
      ['error', "Couldn't add the box: the page has no scan"],
    ])

    hook.unmount()
    await waitFor(() => expect(stop).toHaveBeenCalledTimes(handlers.length))
    tell({ kind: 'locked' })
    expect(shown).toHaveBeenCalledTimes(3)
  })
})
