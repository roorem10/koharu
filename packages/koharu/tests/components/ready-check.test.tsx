import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, expect, it, vi } from 'vitest'

import {
  refreshReadyCheck,
  useReadyCheckDialog,
  useReadySettings,
} from '@/components/ready-check/data'
import { ReadyCheckIndicator } from '@/components/ready-check/ReadyCheckDialog'
import { ReadyCheckPanel } from '@/components/ready-check/ReadyCheckPanel'
import { projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type ReadyCheckItem } from '@koharu/bridge/protocol'

const project = {
  name: 'Chapter 2',
  revision: 0,
  active_page: null,
  can_undo: false,
  can_redo: false,
}
const seriesItem: ReadyCheckItem = {
  status: 'missing',
  reason: 'series_unset',
  data: { kind: 'series', project_name: project.name, series_id: null, series_name: null },
}
const modelsItem: ReadyCheckItem = {
  status: 'missing',
  reason: 'models_missing',
  data: {
    kind: 'models',
    models: [
      { stage: 'ocr', name: 'Manga OCR', downloaded: false, size_bytes: null, cached_bytes: 0 },
    ],
  },
}

beforeEach(() => {
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  queryClient.setQueryData(projectKey, project)
  useReadyCheckDialog.setState({ isOpen: false })
  useReadySettings.setState({ target: null })
})

function show(children: React.ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>)
}

it('routes fixes to their settings page and clears the indicator when the native check becomes ready', async () => {
  const get = vi.spyOn(commands, 'getReadyCheck').mockResolvedValue([
    {
      status: 'missing',
      reason: 'target_language_unset',
      data: { kind: 'target_language', language: '' },
    },
  ])
  show(
    <>
      <ReadyCheckIndicator />
      <ReadyCheckPanel />
    </>,
  )
  const user = userEvent.setup()
  const indicator = await screen.findByRole('button', { name: 'Ready check' })
  await user.click(indicator)
  expect(useReadyCheckDialog.getState().isOpen).toBe(true)
  await user.click(screen.getByRole('button', { name: 'Fix Target language' }))
  expect(useReadySettings.getState().target?.tab).toBe('translation')
  expect(useKoharuStore.getState().settingsOpen).toBe(true)
  expect(useReadyCheckDialog.getState().isOpen).toBe(false)
  const model = { provider: 'openai' as const, model: 'chosen-model' }
  get.mockResolvedValue([
    {
      status: 'missing',
      reason: 'translation_unset',
      data: {
        kind: 'translation',
        model,
        vision: true,
        reading_model: model,
        reading_vision: true,
        reading_enabled: true,
        provider_configured: false,
        reading_provider_configured: false,
      },
    },
  ])
  await act(() => refreshReadyCheck())
  await user.click(
    await screen.findByRole('button', { name: 'Fix Translation and chapter reading' }),
  )
  expect(useReadySettings.getState().target?.tab).toBe('providers')
  get.mockResolvedValue([
    {
      status: 'ready',
      reason: 'target_language_ready',
      data: { kind: 'target_language', language: 'en' },
    },
  ])
  await act(() => refreshReadyCheck())
  await waitFor(() =>
    expect(screen.queryByRole('button', { name: 'Ready check' })).not.toBeInTheDocument(),
  )
})

it('creates the proposed project series only after acceptance and links it through the existing command', async () => {
  vi.spyOn(commands, 'getReadyCheck').mockResolvedValue([seriesItem])
  const create = vi
    .spyOn(commands, 'createSeries')
    .mockResolvedValue({ id: 'series', name: project.name })
  const link = vi.spyOn(commands, 'setProjectSeries').mockResolvedValue(null)
  show(<ReadyCheckPanel />)
  const user = userEvent.setup()
  await user.click(await screen.findByRole('button', { name: 'Fix Project series' }))
  expect(create).not.toHaveBeenCalled()
  await user.click(screen.getByRole('button', { name: /Create series.*Chapter 2/ }))
  await waitFor(() => expect(link).toHaveBeenCalledWith('series'))
  expect(create).toHaveBeenCalledWith(project.name)
})

it('uses the shared model download command and progress, then refreshes its readiness', async () => {
  const get = vi.spyOn(commands, 'getReadyCheck').mockResolvedValue([modelsItem])
  let complete!: (result: null) => void
  const download = vi.spyOn(commands, 'downloadReadyModels').mockImplementation(
    () =>
      new Promise((resolve) => {
        complete = resolve
      }),
  )
  show(<ReadyCheckPanel />)
  await userEvent.setup().click(await screen.findByRole('button', { name: 'Download all' }))
  await waitFor(() => expect(download).toHaveBeenCalledOnce())
  act(() =>
    useKoharuStore.setState({
      downloads: {
        '1': { id: 1, name: 'weights', state: 'running', completed: 50, total: 100, error: null },
      },
    }),
  )
  expect(await screen.findByRole('progressbar', { name: 'weights' })).toHaveAttribute(
    'aria-valuenow',
    '50',
  )
  get.mockResolvedValue([
    {
      ...modelsItem,
      status: 'ready',
      reason: 'models_ready',
      data: {
        kind: 'models',
        models: [
          { stage: 'ocr', name: 'Manga OCR', downloaded: true, size_bytes: 100, cached_bytes: 100 },
        ],
      },
    },
  ])
  await act(async () => complete(null))
  await waitFor(() => expect(screen.getByRole('button', { name: 'Download all' })).toBeDisabled())
})
