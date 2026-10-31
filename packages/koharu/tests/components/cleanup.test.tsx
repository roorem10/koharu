import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, renderHook, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { stepBrush } from '@/components/canvas-tools/brushKeys'
import { useRemoveAreaTool } from '@/components/canvas-tools/removeArea'
import {
  addCleanupArea,
  cleanQueuedAreas,
  clearCleanupAreas,
  settleCleanupJobs,
  syncCleanupProject,
  useCleanupQueue,
} from '@/components/cleanup/queue'
import { cleanupSettingsKey, defaultCleanupSettings } from '@/components/cleanup/settings'
import { CleanupQueueBar } from '@/components/editor/CleanupQueueBar'
import { ToolBar } from '@/components/editor/ToolBar'
import { CleanupPreferences } from '@/components/preferences/CleanupPreferences'
import { useCompareStore } from '@/components/qc/compare'
import { pageKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Job, type Page } from '@koharu/bridge/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

const page: Page = {
  id: 'page',
  label: '01',
  size: { width: 100, height: 200 },
  layers: [],
  regions: [],
}
const stroke = { points: [{ x: 20, y: 30 }], diameter: 20, hardness: 100 }
function wrapper({ children }: { children: React.ReactNode }) {
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>{children}</TooltipProvider>
    </QueryClientProvider>
  )
}
function addStroke(id = page.id) {
  addCleanupArea(id, { kind: 'stroke', stroke })
}
function job(state: Job['state']): Job {
  return {
    id: 'cleanup-job',
    state,
    completed: 1,
    total: 1,
    page: page.id,
    stage: 'inpainting',
    model: 'lama',
    error: null,
    retry: null,
    failures: [],
    project: 'Book',
    operation: null,
    finish_sound: false,
  }
}

beforeEach(() => {
  queryClient.setQueryData(pageKey, page)
  queryClient.setQueryData(projectKey, {
    name: 'Book',
    revision: 1,
    active_page: page.id,
    can_undo: false,
    can_redo: false,
  })
  queryClient.setQueryData(cleanupSettingsKey, { ...defaultCleanupSettings })
  syncCleanupProject('Book')
  useCompareStore.getState().reset()
  useKoharuStore.getState().setTool('remove')
})

describe('cleanup areas', () => {
  it('queues strokes and cleans their union once, retaining areas added during submission', async () => {
    const hook = renderHook(() => useRemoveAreaTool(), { wrapper })
    render(<CleanupQueueBar />, { wrapper })
    await act(async () => {
      await hook.result.current.commitStroke(page.id, 1, stroke.points, stroke.diameter, 0)
      await hook.result.current.commitStroke(page.id, 1, [{ x: 60, y: 90 }], 12, 100)
    })
    expect(screen.getByRole('status')).toHaveTextContent('2 areas queued')
    expect(commands.cleanAreas).not.toHaveBeenCalled()
    let finish!: (value: string) => void
    vi.mocked(commands.cleanAreas).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    fireEvent.click(screen.getByRole('button', { name: 'Clean' }))
    fireEvent.keyDown(window, { key: 'Enter' })
    expect(commands.cleanAreas).toHaveBeenCalledOnce()
    expect(commands.cleanAreas).toHaveBeenCalledWith(
      page.id,
      [
        { ...stroke, hardness: 0 },
        { points: [{ x: 60, y: 90 }], diameter: 12, hardness: 100 },
      ],
      [],
    )
    act(() => addStroke())
    await act(async () => finish('cleanup-job'))
    expect(screen.getByRole('status')).toHaveTextContent('1 area queued')
    act(() => {
      useKoharuStore.setState({ jobs: { 'cleanup-job': job('failed') } })
      settleCleanupJobs()
    })
    expect(screen.getByRole('status')).toHaveTextContent('3 areas queued')
  })

  it('clears without running, and keeps queues with their pages and tools until the project closes', async () => {
    const hook = renderHook(() => useRemoveAreaTool(), { wrapper })
    render(<CleanupQueueBar />, { wrapper })
    act(() => {
      addStroke()
      addStroke('second')
    })
    act(() => queryClient.setQueryData(pageKey, { ...page, id: 'second' }))
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('1 area queued'))
    fireEvent.click(screen.getByRole('button', { name: 'Clear' }))
    expect(useCleanupQueue.getState().pages.second).toEqual([])
    expect(useCleanupQueue.getState().pages.page).toHaveLength(1)
    act(() => {
      queryClient.setQueryData(pageKey, page)
      useKoharuStore.getState().setTool('draw')
    })
    await waitFor(() => expect(screen.getByTestId('cleanup-areas')).toBeInTheDocument())
    fireEvent.keyDown(window, { key: 'Enter' })
    expect(commands.cleanAreas).not.toHaveBeenCalled()
    hook.unmount()
    expect(useCleanupQueue.getState().pages.page).toHaveLength(1)
    act(() => queryClient.setQueryData(projectKey, null))
    expect(useCleanupQueue.getState().pages).toEqual({})
  })

  it('handles Shift-drag rectangles and cancellation, with immediate mode using one run', async () => {
    const { result, rerender } = renderHook(() => useRemoveAreaTool(), { wrapper })
    const start = new PointerEvent('pointerdown', { shiftKey: true })
    const up = new PointerEvent('pointerup')
    act(() => expect(result.current.start({ x: 80, y: 150 }, start)).toBe(true))
    act(() => result.current.move({ x: 10, y: 20 }))
    act(() => result.current.finish(up))
    expect(useCleanupQueue.getState().pages.page[0]).toMatchObject({
      kind: 'rect',
      rect: { x: 10, y: 20, width: 70, height: 130 },
    })
    act(() => result.current.start({ x: 0, y: 0 }, start))
    act(() => result.current.move({ x: 30, y: 50 }))
    act(() => result.current.finish(null))
    expect(useCleanupQueue.getState().pages.page).toHaveLength(1)
    act(() =>
      queryClient.setQueryData(cleanupSettingsKey, {
        ...defaultCleanupSettings,
        remove_mode: 'immediate',
      }),
    )
    rerender()
    await waitFor(() => expect(result.current.settings.remove_mode).toBe('immediate'))
    act(() => result.current.start({ x: 4, y: 8 }, start))
    act(() => result.current.move({ x: 12, y: 16 }))
    act(() => result.current.finish(up))
    await waitFor(() =>
      expect(commands.cleanAreas).toHaveBeenCalledWith(
        page.id,
        [],
        [{ x: 4, y: 8, width: 8, height: 8 }],
      ),
    )
    expect(useCleanupQueue.getState().pages.page).toHaveLength(1)
  })

  it('keeps Enter and Esc local to Remove, ignores text input and comparison, and blocks cleaning during a run', async () => {
    render(
      <>
        <input aria-label='Text' />
        <CleanupQueueBar />
      </>,
      { wrapper },
    )
    act(() => addStroke())
    fireEvent.keyDown(screen.getByLabelText('Text'), { key: 'Escape' })
    expect(useCleanupQueue.getState().pages.page).toHaveLength(1)
    act(() => useCompareStore.setState({ held: true }))
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(useCleanupQueue.getState().pages.page).toHaveLength(1)
    act(() => {
      useCompareStore.getState().reset()
      useKoharuStore.setState({ jobs: { run: { ...job('running'), id: 'run' } } })
    })
    fireEvent.keyDown(window, { key: 'Enter' })
    expect(commands.cleanAreas).not.toHaveBeenCalled()
    act(() => useKoharuStore.setState({ jobs: {} }))
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
    act(() => addStroke())
    fireEvent.keyDown(window, { key: 'Enter' })
    await waitFor(() => expect(commands.cleanAreas).toHaveBeenCalledOnce())
  })

  it('retains the queue when submission fails and clears only the accepted batch on success', async () => {
    addStroke()
    vi.mocked(commands.cleanAreas).mockRejectedValueOnce(new Error('busy'))
    await expect(cleanQueuedAreas(page.id)).rejects.toThrow('busy')
    expect(useCleanupQueue.getState().pages.page).toHaveLength(1)
    await cleanQueuedAreas(page.id)
    expect(useCleanupQueue.getState().pages.page).toEqual([])
    useKoharuStore.setState({ jobs: { 'cleanup-job': job('finished') } })
    settleCleanupJobs()
    expect(useCleanupQueue.getState().pending).toEqual({})
    clearCleanupAreas(page.id)
  })
})

describe('cleanup controls', () => {
  it('remembers each tool and steps size within the page brush limits', () => {
    const store = useKoharuStore.getState()
    store.setTool('draw')
    store.setBrush({ diameter: 20, color: '#FFFFFF' })
    store.setBrushOptions({ hardness: 25, opacity: 50 })
    stepBrush(1)
    expect(useKoharuStore.getState().brush.diameter).toBe(22)
    store.setTool('eraser')
    store.setBrush({ diameter: 10, color: '#FFFFFF' })
    store.setBrushOptions({ hardness: 0 })
    stepBrush(-1)
    expect(useKoharuStore.getState().brush.diameter).toBe(8)
    store.setTool('draw')
    expect(useKoharuStore.getState().brush.diameter).toBe(22)
    expect(useKoharuStore.getState().brushes.draw).toMatchObject({ hardness: 25, opacity: 50 })
    store.setBrush({ diameter: 128, color: '#FFFFFF' })
    stepBrush(1)
    expect(useKoharuStore.getState().brush.diameter).toBe(128)
    store.setBrush({ diameter: 1, color: '#FFFFFF' })
    stepBrush(-1)
    expect(useKoharuStore.getState().brush.diameter).toBe(1)
  })

  it('shows hardness for all brushes and opacity for Brush, and saves cleanup settings independently', async () => {
    act(() => useKoharuStore.getState().setTool('draw'))
    render(
      <>
        <ToolBar />
        <CleanupPreferences />
      </>,
      { wrapper },
    )
    fireEvent.click(screen.getByRole('button', { name: /Brush size:/i }))
    const hardness = await screen.findByRole('group', { name: 'Hardness' })
    const slider = within(hardness).getByRole('slider', { hidden: true })
    expect(slider).toHaveAttribute('aria-labelledby', hardness.getAttribute('aria-labelledby'))
    fireEvent.change(slider, { target: { value: '0' } })
    expect(useKoharuStore.getState().brushes.draw.hardness).toBe(0)
    expect(
      within(screen.getByRole('group', { name: 'Opacity' })).getByRole('slider', { hidden: true }),
    ).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Pixels (0–16)'), { target: { value: '8' } })
    await waitFor(() =>
      expect(commands.saveCleanupSettings).toHaveBeenCalledWith({
        ...defaultCleanupSettings,
        grow_px: 8,
      }),
    )
  })
})
