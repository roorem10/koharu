import { QueryClientProvider } from '@tanstack/react-query'
import { act, renderHook, waitFor } from '@testing-library/react'
import { expect, it, vi } from 'vitest'

import { seriesKey } from '@/components/context/data'
import { useNewChapterSeries } from '@/components/context/NewChapterSeries'
import { queryClient } from '@/lib/queries'
import { commands, type Series } from '@koharu/bridge/protocol'

function wrapper({ children }: { children: React.ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
}

it('offers the last series but keeps a hand choice when its query refreshes', async () => {
  const first = { id: 'first', name: 'First series' }
  const second = { id: 'second', name: 'Second series' }
  vi.spyOn(commands, 'listSeries').mockResolvedValue([first, second])
  const last = vi.spyOn(commands, 'getLastSeries').mockResolvedValue(first)
  const { result } = renderHook(() => useNewChapterSeries(), { wrapper })
  await waitFor(() => expect(result.current.value).toBe('first'))
  act(() => result.current.setValue(null))
  last.mockResolvedValue(second)
  await act(async () => {
    await queryClient.invalidateQueries({ queryKey: seriesKey })
  })
  expect(result.current.value).toBeNull()
  act(() => result.current.setValue('first'))
  expect(result.current.value).toBe('first')
})

it('never offers a deleted series from an old last-used response', async () => {
  let resolve: (series: Series | null) => void = () => undefined
  vi.spyOn(commands, 'getLastSeries').mockReturnValue(
    new Promise((next) => {
      resolve = next
    }),
  )
  vi.spyOn(commands, 'listSeries').mockResolvedValue([])
  const { result } = renderHook(() => useNewChapterSeries(), { wrapper })
  await act(async () => resolve({ id: 'deleted', name: 'Deleted series' }))
  await waitFor(() => expect(result.current.loading).toBe(false))
  expect(result.current.value).toBeNull()
})
