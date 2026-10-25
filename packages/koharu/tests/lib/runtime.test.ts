import { act, render, screen, waitFor } from '@testing-library/react'
import { createElement, StrictMode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import Providers from '@/app/providers'
import { call, refreshShortcuts, saveShortcuts } from '@/lib/backend'
import { useProject } from '@/lib/queries'
import { defaultShortcuts, useKoharuStore } from '@/lib/store'
import {
  commands,
  type Preferences,
  type ProjectInfo,
  type StartupState,
} from '@koharu/bridge/protocol'

vi.mock('@tauri-apps/api/core', () => ({
  Channel: class<T> {
    onmessage: (payload: T) => void

    constructor(handler: (payload: T) => void) {
      this.onmessage = handler
    }
  },
  invoke: vi.fn(),
}))

const preferences: Preferences = {
  pipeline: {
    detect_text_color: true,
    default_text_color: null,
    detection: { model: 'koharu-layout-rfdetr-seg-2xl' },
    ocr: { model: 'paddleocr-vl-1.6' },
    translation: {
      model: {
        provider: 'local',
        model: 'lfm2.5-1.2b-instruct',
        quantization: null,
        vision: false,
        reasoning: false,
      },
      generation: { vision: true, reasoning: true },
      target_language: 'en-US',
      instructions: null,
    },
    inpainting: { model: 'lama' },
    processor: {},
  },
  providers: {
    entries: [],
  },
  typesetting: {
    font_families: ['CCWildWords', 'Adobe 黑体 Std'],
  },
  languages: [],
}

const project: ProjectInfo = {
  name: 'Book',
  revision: 3,
  active_page: null,
  can_undo: true,
  can_redo: false,
}

beforeEach(() => {
  vi.spyOn(commands, 'getTranslationModels').mockResolvedValue([])
  vi.spyOn(commands, 'getShortcuts').mockResolvedValue({})
})

const startupState = (): StartupState => ({
  preferences,
  jobs: [],
  canvas: {
    page: null,
    revision: null,
    generation: 0,
    size: [0, 0],
    element_frames: [],
  },
})

async function start() {
  const pending = deferred<StartupState>()
  const binding = vi.spyOn(commands, 'subscribe').mockReturnValue(pending.promise)
  const view = render(createElement(Providers, null, createElement('div')))
  pending.resolve(startupState())
  await waitFor(() => expect(useKoharuStore.getState().preferences).toBe(preferences))
  return { binding, dispose: view.unmount }
}

function ProjectProbe() {
  const project = useProject().data
  return createElement(
    'span',
    null,
    project === undefined ? 'Loading' : (project?.name ?? 'Closed'),
  )
}

describe('Tauri runtime', () => {
  it('saves shortcut snapshots in edit order', async () => {
    const first = deferred<Record<string, string>>()
    const save = vi
      .spyOn(commands, 'saveShortcuts')
      .mockReturnValueOnce(first.promise)
      .mockResolvedValue({})
    const shortcuts = { ...defaultShortcuts, text: 'q' }
    const older = saveShortcuts(shortcuts)
    shortcuts.text = 'w'
    const newer = saveShortcuts(shortcuts)

    await waitFor(() => expect(save).toHaveBeenCalledTimes(1))
    expect(save.mock.calls[0][0].text).toBe('q')
    first.resolve({})
    await Promise.all([older, newer])
    expect(save.mock.calls.map(([keys]) => keys.text)).toEqual(['q', 'w'])
  })

  it('keeps a newer shortcut edit when the startup response arrives late', async () => {
    const startup = deferred<Record<string, string>>()
    const load = vi.spyOn(commands, 'getShortcuts').mockReturnValue(startup.promise)
    vi.spyOn(commands, 'saveShortcuts').mockResolvedValue({})
    const loading = refreshShortcuts()
    await waitFor(() => expect(load).toHaveBeenCalled())

    useKoharuStore.getState().setShortcut('text', 'q')
    await saveShortcuts(useKoharuStore.getState().shortcuts)
    startup.resolve({ text: 'w' })
    await loading

    expect(useKoharuStore.getState().shortcuts.text).toBe('q')
  })

  it('allows a newer shortcut save after a failed write', async () => {
    const save = vi
      .spyOn(commands, 'saveShortcuts')
      .mockRejectedValueOnce(new Error('write failed'))
      .mockResolvedValue({})
    await expect(saveShortcuts({ ...defaultShortcuts, text: 'q' })).rejects.toThrow('write failed')
    await expect(saveShortcuts({ ...defaultShortcuts, text: 'w' })).resolves.toBeUndefined()
    expect(save.mock.calls.map(([keys]) => keys.text)).toEqual(['q', 'w'])
  })

  it('keeps the project unresolved until its backend query returns', async () => {
    const projectPending = deferred<ProjectInfo | null>()
    vi.spyOn(commands, 'getProject').mockReturnValue(projectPending.promise)
    vi.spyOn(commands, 'subscribe').mockResolvedValue(startupState())
    const view = render(createElement(Providers, null, createElement(ProjectProbe)))

    expect(await screen.findByText('Loading')).toBeInTheDocument()
    projectPending.resolve(null)
    expect(await screen.findByText('Closed')).toBeInTheDocument()
    view.unmount()
  })

  it('keeps one live job channel through Strict Mode effect replay', async () => {
    const binding = vi.spyOn(commands, 'subscribe').mockResolvedValue(startupState())
    const view = render(
      createElement(StrictMode, null, createElement(Providers, null, createElement('div'))),
    )

    await waitFor(() => expect(binding).toHaveBeenCalledTimes(1))
    const [, jobChannel] = binding.mock.calls[0]
    act(() => {
      jobChannel.onmessage({
        id: 'job',
        state: 'running',
        completed: 0,
        total: 4,
        page: 'page',
        stage: 'detection',
        model: 'model',
        error: null,
      })
    })

    expect(useKoharuStore.getState().jobs.job).toMatchObject({ state: 'running', total: 4 })
    view.unmount()
  })

  it('passes only domain arguments to mutation commands', async () => {
    const rename = vi.spyOn(commands, 'renamePage').mockResolvedValue(null)

    await expect(call(commands.renamePage, 'page', 'Chapter 1')).resolves.toBeNull()
    expect(rename).toHaveBeenCalledWith('page', 'Chapter 1')
  })

  it('passes the managed project name to open', async () => {
    const open = vi.spyOn(commands, 'openProject').mockResolvedValue(null)
    await expect(call(commands.openProject, 'Volume 1')).resolves.toBeNull()
    expect(open).toHaveBeenCalledWith('Volume 1')
  })

  it('applies independent channel updates directly to the store', async () => {
    useKoharuStore.setState({ downloads: {}, resources: null })
    const { binding, dispose } = await start()
    const [, , downloadChannel, resourcesChannel] = binding.mock.calls[0]

    downloadChannel.onmessage({
      id: 7,
      state: 'running',
      name: 'model.bin',
      completed: 25,
      total: 100,
      error: null,
    })
    resourcesChannel.onmessage({
      process_memory: 1024,
      system_memory: 8192,
      process_cpu: 5,
      devices: [
        { name: 'GPU', selected: true, memory_budget: 8192, memory_used: 4096, utilization: 40 },
      ],
    })
    expect(useKoharuStore.getState().downloads[7]).toMatchObject({ completed: 25, total: 100 })
    expect(useKoharuStore.getState().resources).toMatchObject({
      process_cpu: 5,
      devices: [{ memory_used: 4096 }],
    })
    dispose()
  })

  it('refreshes project queries when an autonomous job commits work', async () => {
    vi.spyOn(commands, 'getProject').mockResolvedValueOnce(null).mockResolvedValue(project)
    const binding = vi.spyOn(commands, 'subscribe').mockResolvedValue(startupState())
    const view = render(createElement(Providers, null, createElement(ProjectProbe)))
    expect(await screen.findByText('Closed')).toBeInTheDocument()

    const [, jobChannel] = binding.mock.calls[0]
    jobChannel.onmessage({
      id: 'job',
      state: 'running',
      completed: 1,
      total: 2,
      page: 'page',
      stage: 'ocr',
      model: 'model',
      error: null,
    })

    expect(await screen.findByText('Book')).toBeInTheDocument()
    view.unmount()
  })
})

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise
    reject = rejectPromise
  })
  return { promise, resolve, reject }
}
