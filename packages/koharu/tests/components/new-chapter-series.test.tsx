import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react'
import { expect, it, vi } from 'vitest'

import { seriesKey } from '@/components/context/data'
import { NewChapterSeries, useNewChapterSeries } from '@/components/context/NewChapterSeries'
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

it('creates a new series from the picker and selects it', async () => {
  const created = { id: 'new', name: 'New one' }
  const list = vi.spyOn(commands, 'listSeries').mockResolvedValue([])
  vi.spyOn(commands, 'getLastSeries').mockResolvedValue(null)
  const create = vi.spyOn(commands, 'createSeries').mockImplementation(async () => {
    list.mockResolvedValue([created])
    return created
  })
  let value: string | null = null
  function Harness() {
    const choice = useNewChapterSeries()
    value = choice.value
    return <NewChapterSeries choice={choice} />
  }
  render(<Harness />, { wrapper })
  fireEvent.click(await screen.findByRole('button', { name: 'New series' }))
  fireEvent.change(screen.getByRole('textbox', { name: 'Series name' }), {
    target: { value: 'New one' },
  })
  fireEvent.click(screen.getByRole('button', { name: 'Create' }))
  await waitFor(() => expect(create).toHaveBeenCalledWith('New one'))
  await waitFor(() => expect(value).toBe('new'))
  expect(screen.queryByRole('textbox', { name: 'Series name' })).not.toBeInTheDocument()
})
