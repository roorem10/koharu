import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { TitleBar } from '@/components/app/TitleBar'
import { CanvasWorkspace } from '@/components/editor/CanvasWorkspace'
import { useCompareStore } from '@/components/qc/compare'
import { pageKey, pagesKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/bridge/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

const canvas = vi.hoisted(() => ({
  resize: vi.fn(),
  setView: vi.fn(),
  beginTransform: vi.fn(),
  cancelTransform: vi.fn(),
  beginStroke: vi.fn(),
  cancelStroke: vi.fn(),
  updateTransform: vi.fn(),
}))
vi.mock('@/components/editor/useCanvas', () => ({
  useCanvas: () => ({ canvas, status: 'ready', generation: 1, hasFrame: true }),
}))
vi.mock('@koharu/bridge/canvas', () => ({
  prefetchCanvasPages: vi.fn(async () => []),
  workspaceColor: () => [245, 245, 245],
}))

function open() {
  queryClient.setQueryData(projectKey, {
    name: 'Compare test',
    revision: 1,
    active_page: 'page',
    can_undo: false,
    can_redo: false,
  })
  queryClient.setQueryData(pageKey, {
    id: 'page',
    label: '01.png',
    size: { width: 1000, height: 1400 },
    layers: [],
    regions: [],
  })
  queryClient.setQueryData(pagesKey, [])
  useKoharuStore.setState({
    canvasPage: 'page',
    canvasRevision: 1,
    canvasGeneration: 1,
    canvasSize: [1000, 1400],
  })
  render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <input aria-label='Translation input' />
        <CanvasWorkspace />
      </TooltipProvider>
    </QueryClientProvider>,
  )
  const surface = screen.getByLabelText('Koharu canvas')
  Object.defineProperty(surface, 'getBoundingClientRect', {
    value: () => ({ x: 0, y: 0, width: 800, height: 600 }),
  })
  return surface
}

afterEach(() => vi.unstubAllGlobals())

const press = { key: '\\', code: 'Backslash' }

describe('original page comparison', () => {
  it('holds the shortcut to show the source at the same camera position, then releases it', async () => {
    vi.stubGlobal('devicePixelRatio', 2)
    const surface = open()
    act(() =>
      useKoharuStore.setState({ camera: { zoom: 2, translation: [40, 60], fitted: false } }),
    )
    fireEvent.keyDown(surface, press)
    const image = await screen.findByRole('img', { name: 'Original' })
    expect(commands.getPageSource).toHaveBeenCalledWith('page')
    expect(image).toHaveStyle({ transform: 'translate(20px, 30px) scale(1)' })
    act(() =>
      queryClient.setQueryData(pageKey, (page: object) => ({ ...page, label: 'Refreshed page' })),
    )
    expect(screen.getByTestId('original-view')).toBeInTheDocument()
    fireEvent.keyUp(window, press)
    expect(screen.queryByTestId('original-view')).not.toBeInTheDocument()
    expect(useKoharuStore.getState().canvasRevision).toBe(1)
    expect(queryClient.getQueryData(projectKey)).toMatchObject({ revision: 1, can_undo: false })
    expect(URL.revokeObjectURL).toHaveBeenCalled()
  })

  it('pins with the button and leaves the pin in place after releasing a held key', async () => {
    const surface = open()
    const toggle = screen.getByRole('button', { name: 'Show original' })
    fireEvent.click(toggle)
    expect(toggle).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByTestId('original-view')).toBeInTheDocument()
    fireEvent.keyDown(surface, press)
    fireEvent.keyUp(window, press)
    expect(screen.getByTestId('original-view')).toBeInTheDocument()
    fireEvent.click(toggle)
    expect(screen.queryByTestId('original-view')).not.toBeInTheDocument()
  })

  it('shares the pinned state with View > Show original', async () => {
    open()
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <TitleBar />
      </QueryClientProvider>,
    )
    await user.click(screen.getByRole('menuitem', { name: 'View' }))
    await user.click(await screen.findByRole('menuitem', { name: /Show original/ }))
    expect(screen.getByTestId('original-view')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Show original' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
  })

  it('keeps text-entry keys and supports a rebound shortcut with release after modifiers change', async () => {
    const surface = open()
    fireEvent.keyDown(screen.getByRole('textbox', { name: 'Translation input' }), press)
    expect(screen.queryByTestId('original-view')).not.toBeInTheDocument()
    act(() => useKoharuStore.getState().setShortcut('compare', 'alt+q'))
    fireEvent.keyDown(surface, { key: 'q', code: 'KeyQ', altKey: true })
    expect(screen.getByTestId('original-view')).toBeInTheDocument()
    fireEvent.keyUp(window, { key: 'q', code: 'KeyQ' })
    expect(screen.queryByTestId('original-view')).not.toBeInTheDocument()
  })

  it('pauses pointer edits, Delete and Undo, and releases a held comparison on blur', async () => {
    const surface = open()
    vi.spyOn(commands, 'deleteLayers').mockResolvedValue(null)
    vi.spyOn(commands, 'undo').mockResolvedValue(null)
    vi.spyOn(commands, 'addPointText')
    act(() => useKoharuStore.setState({ tool: 'text', selectedLayers: ['box'] }))
    fireEvent.keyDown(surface, press)
    fireEvent.pointerDown(surface, { pointerId: 1, button: 0, clientX: 30, clientY: 40 })
    fireEvent.pointerUp(surface, { pointerId: 1, button: 0, clientX: 30, clientY: 40 })
    fireEvent.keyDown(surface, { key: 'Delete' })
    fireEvent.keyDown(surface, { key: 'z', code: 'KeyZ', ctrlKey: true })
    expect(commands.addPointText).not.toHaveBeenCalled()
    expect(commands.deleteLayers).not.toHaveBeenCalled()
    expect(commands.undo).not.toHaveBeenCalled()
    fireEvent.blur(window)
    await waitFor(() => expect(screen.queryByTestId('original-view')).not.toBeInTheDocument())
    expect(useCompareStore.getState().held).toBe(false)
  })
})
