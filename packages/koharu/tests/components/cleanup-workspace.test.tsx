import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { useCleanupQueue } from '@/components/cleanup/queue'
import { cleanupSettingsKey, defaultCleanupSettings } from '@/components/cleanup/settings'
import { CanvasWorkspace } from '@/components/editor/CanvasWorkspace'
import { pageKey, pagesKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Page } from '@koharu/bridge/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

const canvas = vi.hoisted(() => ({
  resize: vi.fn(),
  setView: vi.fn(),
  beginStroke: vi.fn(),
  extendStroke: vi.fn(),
  finishStroke: vi.fn(),
  cancelStroke: vi.fn(),
  cancelTransform: vi.fn(),
}))
vi.mock('@/components/editor/useCanvas', () => ({
  useCanvas: () => ({ canvas, status: 'ready', generation: 1, hasFrame: true, error: null }),
}))
vi.mock('@koharu/bridge/canvas', () => ({
  prefetchCanvasPages: vi.fn(async () => []),
  workspaceColor: () => [245, 245, 245],
}))

function open(layers: Page['layers'] = []) {
  const page: Page = {
    id: 'page',
    label: 'Page',
    size: { width: 1000, height: 1000 },
    layers,
    regions: [],
  }
  const project = {
    name: 'Book',
    revision: 1,
    active_page: page.id,
    can_undo: false,
    can_redo: false,
  }
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pageKey, page)
  queryClient.setQueryData(pagesKey, [])
  queryClient.setQueryData(cleanupSettingsKey, { ...defaultCleanupSettings })
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  vi.spyOn(commands, 'getPage').mockResolvedValue(page)
  vi.spyOn(commands, 'getPages').mockResolvedValue([])
  useKoharuStore.setState({
    canvasPage: page.id,
    canvasRevision: 1,
    canvasGeneration: 1,
    canvasSize: [1000, 1000],
  })
  render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <CanvasWorkspace />
      </TooltipProvider>
    </QueryClientProvider>,
  )
  const surface = screen.getByLabelText('Koharu canvas')
  Object.defineProperty(surface, 'getBoundingClientRect', {
    value: () => ({ x: 0, y: 0, width: 1000, height: 1000 }),
  })
  return surface
}
function mark(surface: HTMLElement, pointerId: number, shiftKey = false) {
  fireEvent.pointerDown(surface, { button: 0, pointerId, clientX: 20, clientY: 30, shiftKey })
  fireEvent.pointerMove(surface, { pointerId, clientX: 40, clientY: 50, shiftKey })
  fireEvent.pointerUp(surface, { pointerId, clientX: 40, clientY: 50, shiftKey })
}

describe('cleanup canvas integration', () => {
  it('collects two Remove strokes and a Shift rectangle for one Clean invocation', async () => {
    useKoharuStore.getState().setTool('remove')
    const inpaint = vi.spyOn(commands, 'commitInpaint')
    const surface = open()
    mark(surface, 1)
    await waitFor(() => expect(useCleanupQueue.getState().pages.page).toHaveLength(1))
    mark(surface, 2)
    await waitFor(() => expect(useCleanupQueue.getState().pages.page).toHaveLength(2))
    mark(surface, 3, true)
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('3 areas queued'))
    expect(inpaint).not.toHaveBeenCalled()
    expect(canvas.beginStroke).toHaveBeenCalledTimes(2)
    fireEvent.keyDown(surface, { key: 'Enter' })
    await waitFor(() => expect(commands.cleanAreas).toHaveBeenCalledOnce())
    expect(vi.mocked(commands.cleanAreas).mock.calls[0][1]).toHaveLength(2)
    expect(vi.mocked(commands.cleanAreas).mock.calls[0][2]).toEqual([
      { x: 20, y: 30, width: 20, height: 20 },
    ])
  })

  it('sends Brush hardness and opacity captured at pointer-down', async () => {
    useKoharuStore.getState().setTool('draw')
    useKoharuStore.getState().setBrushOptions({ hardness: 0, opacity: 25 })
    const paint = vi
      .spyOn(commands, 'commitPaint')
      .mockResolvedValue({ revision: 2, layer: 'cleanup' })
    const surface = open()
    fireEvent.pointerDown(surface, { button: 0, pointerId: 1, clientX: 20, clientY: 30 })
    act(() => useKoharuStore.getState().setBrushOptions({ hardness: 100, opacity: 100 }))
    fireEvent.pointerMove(surface, { pointerId: 1, clientX: 40, clientY: 50 })
    fireEvent.pointerUp(surface, { pointerId: 1, clientX: 40, clientY: 50 })
    await waitFor(() => expect(paint).toHaveBeenCalledOnce())
    expect(paint).toHaveBeenCalledWith(1, null, expect.any(Array), {
      diameter: 48,
      hardness: 0,
      color: [255, 255, 255, 64],
    })
  })

  it.each([false, true])(
    'targets Cleanup for unselected Eraser strokes (existing layer: %s)',
    async (existing) => {
      useKoharuStore.getState().setTool('eraser')
      useKoharuStore.getState().setBrushOptions({ hardness: 15 })
      const erase = vi
        .spyOn(commands, 'commitErase')
        .mockResolvedValue({ revision: 2, layer: 'cleanup' })
      const surface = open(
        existing
          ? [
              {
                type: 'raster',
                id: 'cleanup',
                parent: 'page',
                kind: 'cleanup',
                name: 'Cleanup',
                image: null,
                visibility: { visible: true, opacity: 1 },
              },
            ]
          : [],
      )
      mark(surface, 1)
      await waitFor(() => expect(erase).toHaveBeenCalledOnce())
      expect(erase).toHaveBeenCalledWith(1, existing ? 'cleanup' : null, expect.any(Array), 48, 15)
    },
  )

  it('uses the bracket keys on the canvas without taking them from text entry', () => {
    useKoharuStore.getState().setTool('remove')
    const surface = open()
    fireEvent.keyDown(surface, { key: ']', code: 'BracketRight' })
    expect(useKoharuStore.getState().brush.diameter).toBe(53)
    fireEvent.keyDown(surface, { key: '[', code: 'BracketLeft' })
    expect(useKoharuStore.getState().brush.diameter).toBe(48)
    const input = document.createElement('input')
    document.body.append(input)
    fireEvent.keyDown(input, { key: ']', code: 'BracketRight' })
    expect(useKoharuStore.getState().brush.diameter).toBe(48)
    input.remove()
    act(() => useKoharuStore.getState().setTool('select'))
    fireEvent.keyDown(surface, { key: ']', code: 'BracketRight' })
    expect(useKoharuStore.getState().brush.diameter).toBe(48)
  })
})
