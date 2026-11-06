import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'

import { ActivityCenter } from '@/components/editor/ActivityCenter'
import { RunPreferences } from '@/components/preferences/RunPreferences'
import { AutoRunDialog } from '@/components/runs/AutoRun'
import { BatchChaptersDialog } from '@/components/runs/BatchChaptersDialog'
import { projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import translations from '@/public/locales/en-US/translation.json'
import { commands, type AutoRunPreview, type Job } from '@koharu/bridge/protocol'

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
const preview: AutoRunPreview = {
  quality: 'balanced',
  pages: 12,
  can_read_images: false,
  estimated_requests: 12,
  steps: [
    { id: 'detection', needed: true, reason: null },
    { id: 'read_chapter', needed: false, reason: 'no_vision' },
    { id: 'translation', needed: true, reason: null },
  ],
}
function show(children: React.ReactNode) {
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  queryClient.setQueryData(projectKey, project)
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>)
}

it('waits for the native plan and starts the confirmed scope without a second chapter-reading request', async () => {
  let resolve!: (value: AutoRunPreview) => void
  const plan = vi.spyOn(commands, 'previewAutoRun').mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done
      }),
  )
  const run = vi.spyOn(commands, 'autoRun').mockResolvedValue('job')
  const close = vi.fn()
  const scope = { scope: 'pages' as const, value: ['p2', 'p3'] }
  show(<AutoRunDialog open onOpenChange={close} scope={scope} />)
  const user = userEvent.setup()
  const start = screen.getByRole('button', { name: translations.autoRun.start })
  expect(start).toBeDisabled()
  await waitFor(() => expect(plan).toHaveBeenCalledWith(scope, 'balanced'))
  await act(async () => resolve(preview))
  await waitFor(() => expect(start).toBeEnabled())
  expect(run).not.toHaveBeenCalled()
  expect(screen.getByText(translations.autoRun.visionNo)).toBeInTheDocument()
  await user.click(start)
  await waitFor(() => expect(run).toHaveBeenCalledExactlyOnceWith(scope, 'balanced'))
  expect(commands.readChapter).not.toHaveBeenCalled()
  expect(close).toHaveBeenCalledWith(false)
})

it('cancels the confirmation without starting work', async () => {
  vi.spyOn(commands, 'previewAutoRun').mockResolvedValue(preview)
  const run = vi.spyOn(commands, 'autoRun').mockResolvedValue('job')
  const close = vi.fn()
  show(<AutoRunDialog open onOpenChange={close} scope={{ scope: 'project' }} />)
  await userEvent.setup().click(screen.getByRole('button', { name: translations.runs.cancel }))
  expect(close).toHaveBeenCalledWith(false)
  expect(run).not.toHaveBeenCalled()
})

it('persists quality while retaining retries, manual presets and optional recipe settings', async () => {
  const save = vi.spyOn(commands, 'saveRunSettings').mockImplementation(async (value) => value)
  show(<RunPreferences />)
  const user = userEvent.setup()
  await user.click(await screen.findByRole('combobox', { name: translations.autoRun.quality }))
  await user.click(await screen.findByRole('option', { name: translations.autoRun.qualities.best }))
  await waitFor(() => expect(save).toHaveBeenCalledOnce())
  expect(save.mock.lastCall?.[0]).toMatchObject({
    auto_run_quality: 'best',
    auto_run_fix_levels: false,
    auto_run_export: false,
    retries: 2,
    order: 'overlapped',
    presets: expect.any(Array),
  })
})

it('uses the native batch preview and one combined close-and-start confirmation', async () => {
  vi.spyOn(commands, 'previewChapterBatch').mockResolvedValue({
    id: 'plan',
    chapters: [{ id: 'one', name: 'One', pages: 12, error: null }],
  })
  const plan = vi.spyOn(commands, 'previewBatchAutoRun').mockResolvedValue(preview)
  const run = vi.spyOn(commands, 'runChapterBatch').mockResolvedValue(null)
  show(<BatchChaptersDialog open onOpenChange={vi.fn()} />)
  const user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: translations.runs.pickFolder }))
  await user.click(await screen.findByRole('combobox', { name: translations.runs.batchPreset }))
  await user.click(
    await screen.findByRole('option', {
      name: translations.autoRun.button.replace(
        '{{quality}}',
        translations.autoRun.qualities.balanced,
      ),
    }),
  )
  await user.click(screen.getByRole('button', { name: translations.runs.startBatch }))
  await waitFor(() => expect(plan).toHaveBeenCalledWith('plan', ['one'], 'balanced'))
  expect(run).not.toHaveBeenCalled()
  const start = screen.getByRole('button', { name: translations.runs.closeAndStart })
  await waitFor(() => expect(start).toBeEnabled())
  await user.click(start)
  await waitFor(() => expect(run).toHaveBeenCalledOnce())
  expect(run.mock.lastCall?.slice(0, 5)).toEqual([
    'plan',
    ['one'],
    { recipe: 'auto_run', quality: 'balanced' },
    null,
    'chapter',
  ])
})

it('retains the completed recipe summary with skip reasons and dismisses it through the existing job', async () => {
  const job: Job = {
    id: 'job',
    state: 'finished',
    completed: 12,
    total: 12,
    page: null,
    stage: null,
    model: null,
    error: null,
    retry: null,
    failures: [],
    project: 'chapter',
    operation: null,
    finish_sound: true,
    auto_run: {
      step: null,
      summary: {
        pages: 12,
        failures: 0,
        elapsed_seconds: 25,
        steps: [{ id: 'read_chapter', status: 'skipped', reason: 'no_vision', error: null }],
      },
    },
  }
  useKoharuStore.setState({
    jobs: { job: { ...job, state: 'running', auto_run: { step: 'read_chapter', summary: null } } },
  })
  show(<ActivityCenter />)
  expect(screen.getByText(translations.autoRun.steps.read_chapter)).toBeInTheDocument()
  act(() => useKoharuStore.setState({ jobs: { job } }))
  expect(screen.getByRole('status')).toBeInTheDocument()
  expect(screen.getByText(new RegExp(translations.autoRun.reasons.no_vision))).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: translations.activity.stop })).not.toBeInTheDocument()
  await userEvent.setup().click(screen.getByRole('button', { name: translations.activity.dismiss }))
  expect(useKoharuStore.getState().jobs.job).toBeUndefined()
})

it('retries a failed recipe step through the existing job retry command without pipeline failures', async () => {
  const retry = vi.spyOn(commands, 'retryFailedPages').mockResolvedValue('retry')
  const job: Job = {
    id: 'job',
    state: 'finished_with_problems',
    completed: 12,
    total: 12,
    page: null,
    stage: null,
    model: null,
    error: null,
    retry: null,
    failures: [],
    project: 'chapter',
    operation: null,
    finish_sound: false,
    auto_run: {
      step: null,
      summary: {
        pages: 12,
        failures: 1,
        elapsed_seconds: 25,
        steps: [{ id: 'read_chapter', status: 'failed', reason: null, error: 'Reader failed' }],
      },
    },
  }
  useKoharuStore.setState({ jobs: { job } })
  show(<ActivityCenter />)
  await userEvent
    .setup()
    .click(screen.getByRole('button', { name: translations.runs.retryFailedPages }))
  await waitFor(() => expect(retry).toHaveBeenCalledExactlyOnceWith('job'))
})
