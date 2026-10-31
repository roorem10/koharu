import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'

import { ActivityCenter } from '@/components/editor/ActivityCenter'
import { pageKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Job } from '@koharu/bridge/protocol'
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
function job(overrides: Partial<Job> = {}): Job {
  return {
    id: 'job',
    state: 'running',
    completed: 0,
    total: 2,
    page: null,
    stage: null,
    model: null,
    error: null,
    retry: null,
    failures: [],
    project: 'chapter',
    operation: { operation: 'full' },

    ...overrides,
  }
}

it('retries a job and navigates through the existing page selection and query cache', async () => {
  const retry = vi.spyOn(commands, 'retryFailedPages').mockResolvedValue('retry')
  vi.spyOn(commands, 'getPages').mockResolvedValue([])
  const page = {
    id: 'failed',
    label: 'A very long chapter page filename that must remain readable.png',
    size: { width: 100, height: 100 },
    layers: [],
    regions: [],
  }
  const select = vi
    .spyOn(commands, 'selectPage')
    .mockResolvedValue({ project: { ...project, active_page: page.id }, page })
  useKoharuStore.setState({
    jobs: {
      job: job({
        state: 'finished_with_problems',
        failures: [
          {
            page: page.id,
            label: page.label,
            stage: 'translation',
            error: 'returned 1 segments; expected 2',
          },
        ],
      }),
    },
  })
  show(<ActivityCenter />)
  const user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: page.label }))
  await waitFor(() => expect(select).toHaveBeenCalledWith('failed'))
  expect(useKoharuStore.getState().selectedPages).toEqual(['failed'])
  expect(queryClient.getQueryData(pageKey)).toEqual(page)
  await user.click(screen.getByRole('button', { name: 'Retry failed pages' }))
  await waitFor(() => expect(retry).toHaveBeenCalledWith('job'))
})
