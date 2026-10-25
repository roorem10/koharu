import { QueryClientProvider } from '@tanstack/react-query'
import { PhysicalPosition } from '@tauri-apps/api/dpi'
import type { DragDropEvent } from '@tauri-apps/api/webview'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { FileDropImport } from '@/components/app/FileDropImport'
import { projectKey, queryClient, useImportPages } from '@/lib/queries'
import { commands } from '@koharu/bridge/protocol'
import { toast } from '@koharu/ui/components/toast'

const { listen } = vi.hoisted(() => ({ listen: vi.fn() }))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ onDragDropEvent: listen }),
}))

const project = {
  name: 'Volume 1',
  revision: 1,
  active_page: null,
  can_undo: false,
  can_redo: false,
}
const position = new PhysicalPosition(20, 30)
let receive: (event: { payload: DragDropEvent }) => void

function ImportStatus() {
  const { importing } = useImportPages()
  return <span data-testid='importing'>{String(importing)}</span>
}

function mount() {
  return render(
    <QueryClientProvider client={queryClient}>
      <FileDropImport />
      <ImportStatus />
    </QueryClientProvider>,
  )
}

async function emit(payload: DragDropEvent) {
  await act(async () => receive({ payload }))
}

beforeEach(() => {
  queryClient.setQueryData(projectKey, project)
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  vi.spyOn(commands, 'getPages').mockResolvedValue([])
  vi.spyOn(commands, 'getPage').mockResolvedValue(null)
  listen.mockImplementation(async (callback) => {
    receive = callback
    return vi.fn()
  })
})

describe('native file-drop import', () => {
  it.each(['dragenter', 'dragover'])(
    'accepts %s carrying Files without reading file contents',
    (type) => {
      const contents = vi.fn(() => {
        throw new Error('file contents must not be read')
      })
      const dataTransfer = {
        types: ['Files'],
        dropEffect: 'none',
        get files() {
          return contents()
        },
        get items() {
          return contents()
        },
      }
      mount()
      const event = new Event(type, { cancelable: true })
      Object.defineProperty(event, 'dataTransfer', { value: dataTransfer })
      fireEvent(window, event)
      expect(event.defaultPrevented).toBe(true)
      expect(dataTransfer.dropEffect).toBe('copy')
      expect(contents).not.toHaveBeenCalled()
    },
  )

  it.each(['dragenter', 'dragover'])(
    'leaves %s without Files available to internal drags',
    (type) => {
      const dataTransfer = { types: ['application/x-koharu-pages'], dropEffect: 'move' }
      mount()
      const event = new Event(type, { cancelable: true })
      Object.defineProperty(event, 'dataTransfer', { value: dataTransfer })
      fireEvent(window, event)
      expect(event.defaultPrevented).toBe(false)
      expect(dataTransfer.dropEffect).toBe('move')
    },
  )

  it('clears the overlay on leave, Escape, blur and DOM cancellation without importing', async () => {
    const dropped = vi.spyOn(commands, 'importDropped').mockResolvedValue(0)
    mount()
    await waitFor(() => expect(listen).toHaveBeenCalled())
    await emit({ type: 'enter', paths: ['D:\\page.png'], position })
    expect(screen.getByRole('status')).toBeInTheDocument()
    await emit({ type: 'leave' })
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
    await emit({ type: 'over', position })
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
    await emit({ type: 'over', position })
    fireEvent.blur(window)
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
    await emit({ type: 'over', position })
    const drop = new Event('drop', { cancelable: true })
    fireEvent(window, drop)
    expect(drop.defaultPrevented).toBe(true)
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
    expect(dropped).not.toHaveBeenCalled()
  })

  it('uses native paths once, shares import progress and refreshes queries with the skipped count', async () => {
    let finish!: (result: number) => void
    const dropped = vi.spyOn(commands, 'importDropped').mockReturnValue(
      new Promise((resolve) => {
        finish = resolve
      }),
    )
    const notice = vi.spyOn(toast, 'add')
    const getProject = vi.mocked(commands.getProject)
    mount()
    await waitFor(() => expect(listen).toHaveBeenCalled())
    const paths = ['D:\\03.png', 'D:\\01.png', 'D:\\猫02.jpg']
    await emit({ type: 'enter', paths, position })
    await emit({ type: 'drop', paths, position })
    await emit({ type: 'drop', paths, position })
    await waitFor(() => expect(screen.getByTestId('importing')).toHaveTextContent('true'))
    expect(dropped).toHaveBeenCalledExactlyOnceWith(paths)
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
    await act(async () => finish(2))
    await waitFor(() => expect(screen.getByTestId('importing')).toHaveTextContent('false'))
    expect(getProject).toHaveBeenCalledOnce()
    expect(notice).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'info',
        title: expect.stringContaining('2'),
      }),
    )
  })

  it('uses current project state without registering another native listener', async () => {
    const dropped = vi.spyOn(commands, 'importDropped').mockResolvedValue(0)
    const notice = vi.spyOn(toast, 'add')
    mount()
    await waitFor(() => expect(listen).toHaveBeenCalledOnce())
    await act(async () => {
      queryClient.setQueryData(projectKey, null)
    })
    await emit({ type: 'enter', paths: ['D:\\page.png'], position })
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
    await emit({ type: 'drop', paths: ['D:\\page.png'], position })
    expect(dropped).not.toHaveBeenCalled()
    expect(notice).toHaveBeenCalledWith(expect.objectContaining({ type: 'info' }))
    expect(listen).toHaveBeenCalledOnce()
  })

  it('clears pending state after a native refusal and can import again', async () => {
    const dropped = vi
      .spyOn(commands, 'importDropped')
      .mockRejectedValueOnce(new Error('pages cannot be imported while processing is running'))
      .mockResolvedValueOnce(0)
    const notice = vi.spyOn(toast, 'add')
    mount()
    await waitFor(() => expect(listen).toHaveBeenCalled())
    const drop: DragDropEvent = { type: 'drop', paths: ['D:\\page.png'], position }
    await emit(drop)
    await waitFor(() => expect(screen.getByTestId('importing')).toHaveTextContent('false'))
    await emit(drop)
    await waitFor(() => expect(dropped).toHaveBeenCalledTimes(2))
    expect(notice).toHaveBeenCalledWith(expect.objectContaining({ type: 'error' }))
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
  })

  it('releases a listener that resolves after unmount and removes DOM cancellation', async () => {
    let connected!: (stop: () => void) => void
    listen.mockReturnValue(
      new Promise((resolve) => {
        connected = resolve
      }),
    )
    const stop = vi.fn()
    const view = mount()
    view.unmount()
    await act(async () => connected(stop))
    expect(stop).toHaveBeenCalledOnce()
    const drop = new Event('drop', { cancelable: true })
    fireEvent(window, drop)
    expect(drop.defaultPrevented).toBe(false)
    for (const type of ['dragenter', 'dragover']) {
      const drag = new Event(type, { cancelable: true })
      Object.defineProperty(drag, 'dataTransfer', {
        value: { types: ['Files'], dropEffect: 'none' },
      })
      fireEvent(window, drag)
      expect(drag.defaultPrevented).toBe(false)
    }
  })
})
