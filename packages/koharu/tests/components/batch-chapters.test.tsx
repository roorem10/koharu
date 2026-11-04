import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'

import { BatchChaptersDialog } from '@/components/runs/BatchChaptersDialog'
import { projectKey, queryClient } from '@/lib/queries'
import { commands } from '@koharu/bridge/protocol'
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  Channel: class<T> {
    onmessage: (payload: T) => void = () => undefined
  },
}))
const project = {
  name: 'chapter',
  revision: 1,
  active_page: 'page',
  can_undo: false,
  can_redo: false,
}
function show(children: React.ReactNode) {
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  queryClient.setQueryData(projectKey, project)
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>)
}
it('previews chapters, allows exclusions, and explicitly closes the open project before starting', async () => {
  vi.spyOn(commands, 'previewChapterBatch').mockResolvedValue({
    id: 'plan',
    chapters: [
      { id: 'one', name: 'Chapter 1', pages: 2, error: null },
      { id: 'two', name: 'Chapter 2', pages: 3, error: null },
    ],
  })
  const run = vi.spyOn(commands, 'runChapterBatch').mockResolvedValue(null)
  show(<BatchChaptersDialog open onOpenChange={vi.fn()} />)
  const user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: 'Pick a folder' }))
  const second = await screen.findByRole('checkbox', { name: /Chapter 2/ })
  await user.click(second)
  await user.click(screen.getByRole('button', { name: 'Start' }))
  expect(run).not.toHaveBeenCalled()
  await user.click(screen.getByRole('button', { name: 'Close project and start' }))
  await waitFor(() => expect(run).toHaveBeenCalledOnce())
  expect(run.mock.lastCall?.slice(0, 5)).toEqual(['plan', ['one'], 0, 'chapter', expect.anything()])
  act(() =>
    run.mock.lastCall?.[4].onmessage({
      chapters: [],
      running: false,
      stopped: false,
      finish_sound: false,
    }),
  )
})
