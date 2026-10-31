import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ProcessRedoItems } from '@/components/app/ProcessRedoItems'
import { useOcrBoxTool } from '@/components/canvas-tools/ocrBox'
import { BoxRedoSection } from '@/components/ocr-box/BoxRedoSection'
import { ocrBoxSettingsKey } from '@/components/ocr-box/queries'
import { OcrBoxPreferences } from '@/components/preferences/OcrBoxPreferences'
import { useCompareStore } from '@/components/qc/compare'
import { pageKey, pagesKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Layer, type OcrBoxMode, type OcrBoxSettings } from '@koharu/bridge/protocol'
import { Menubar, MenubarContent, MenubarMenu, MenubarTrigger } from '@koharu/ui/components/menubar'

const settings: OcrBoxSettings = {
  default_mode: 'read_translate',
  subtitle_font_family: null,
  subtitle_size: 18,
  subtitle_fill: [0, 0, 0, 255],
  subtitle_outline: [255, 255, 255, 255],
}
const textLayer: Layer = {
  type: 'text',
  id: 'box',
  parent: null,
  geometry: null,
  angle_degrees: null,
  visibility: { visible: true, opacity: 1 },
  content: { id: 'content', source: null, translation: null, role: null, source_region: null },
  typography: null,
  layout: 'paragraph',
  automatic_region: null,
}
const page = {
  id: 'page',
  label: 'Page',
  size: { width: 500, height: 400 },
  layers: [textLayer],
  regions: [],
}
const project = { name: 'OCR', revision: 1, active_page: 'page', can_undo: false, can_redo: false }

function wrapper({ children }: { children: React.ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
}
function pointer(modifiers: MouseEventInit = {}) {
  return new MouseEvent('pointerup', { button: 0, ...modifiers }) as PointerEvent
}

beforeEach(() => {
  useCompareStore.getState().reset()
  vi.spyOn(commands, 'getPage').mockResolvedValue(page)
  vi.spyOn(commands, 'getPages').mockResolvedValue([])
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  vi.spyOn(commands, 'getFonts').mockResolvedValue([])
  queryClient.setQueryData(pageKey, page)
  queryClient.setQueryData(pagesKey, [])
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(ocrBoxSettingsKey, settings)
  useKoharuStore.setState({ canvasPage: 'page', tool: 'ocr_box' })
})

describe('OCR box drag', () => {
  it.each([
    [{}, 'read_translate'],
    [{ altKey: true }, 'read'],
    [{ shiftKey: true }, 'read_translate_clean'],
    [{ altKey: true, shiftKey: true }, 'read'],
  ] as const)('uses the release modifiers %j', async (modifiers, mode) => {
    const hook = renderHook(() => useOcrBoxTool(), { wrapper })
    // CanvasWorkspace holds the handler returned at pointer-down for the whole drag.
    const handler = hook.result.current
    act(() => {
      expect(handler.start({ x: 90, y: 100 }, pointer())).toBe(true)
    })
    act(() => handler.move({ x: 10, y: 20 }))
    act(() => handler.finish(pointer(modifiers)))
    await waitFor(() =>
      expect(commands.addOcrBox).toHaveBeenCalledWith(
        'page',
        { x: 10, y: 20, width: 80, height: 80 },
        mode,
      ),
    )
    await waitFor(() => expect(useKoharuStore.getState().selectedLayers).toEqual(['ocr-box']))
  })

  it('honors the saved default and clips a drag to page pixels', async () => {
    queryClient.setQueryData(ocrBoxSettingsKey, { ...settings, default_mode: 'read' as OcrBoxMode })
    const hook = renderHook(() => useOcrBoxTool(), { wrapper })
    const handler = hook.result.current
    act(() => {
      handler.start({ x: 480, y: 380 }, pointer())
      handler.move({ x: 700, y: 500 })
    })
    act(() => handler.finish(pointer()))
    await waitFor(() =>
      expect(commands.addOcrBox).toHaveBeenCalledWith(
        'page',
        { x: 480, y: 380, width: 20, height: 20 },
        'read',
      ),
    )
  })

  it('ignores tiny drags at the current screen scale and cancelled gestures', () => {
    useKoharuStore.setState({ camera: { zoom: 0.5, translation: [0, 0], fitted: true } })
    const hook = renderHook(() => useOcrBoxTool(), { wrapper })
    const handler = hook.result.current
    act(() => {
      handler.start({ x: 10, y: 20 }, pointer())
      handler.move({ x: 16, y: 26 })
      handler.finish(pointer())
    })
    act(() => {
      handler.start({ x: 10, y: 20 }, pointer())
      handler.move({ x: 60, y: 80 })
      handler.finish(null)
    })
    expect(commands.addOcrBox).not.toHaveBeenCalled()
  })

  it('cancels when the page changes or the original is showing', () => {
    const hook = renderHook(() => useOcrBoxTool(), { wrapper })
    const handler = hook.result.current
    act(() => {
      handler.start({ x: 10, y: 20 }, pointer())
      handler.move({ x: 60, y: 80 })
    })
    act(() => useCompareStore.getState().setHeld(true))
    act(() => handler.finish(pointer()))
    expect(commands.addOcrBox).not.toHaveBeenCalled()
    expect(handler.start({ x: 10, y: 20 }, pointer())).toBe(false)
    act(() => useCompareStore.getState().reset())
    act(() => {
      handler.start({ x: 10, y: 20 }, pointer())
      handler.move({ x: 60, y: 80 })
    })
    act(() => queryClient.setQueryData(pageKey, { ...page, id: 'different-page' }))
    hook.rerender()
    act(() => handler.finish(pointer()))
    expect(commands.addOcrBox).not.toHaveBeenCalled()
  })

  it('does not take a gesture while a job is running', () => {
    useKoharuStore.setState({
      jobs: {
        job: {
          id: 'job',
          state: 'running',
          completed: 0,
          total: 1,
          page: 'page',
          stage: 'ocr',
          model: null,
          error: null,
          retry: null,
          failures: [],
          project: 'OCR',
          operation: null,
          finish_sound: false,
        },
      },
    })
    const hook = renderHook(() => useOcrBoxTool(), { wrapper })
    expect(hook.result.current.start({ x: 10, y: 20 }, pointer())).toBe(false)
  })
})

describe('box redo controls', () => {
  it.each([
    ['Read again', 'read'],
    ['Translate again', 'translate'],
    ['Clean again', 'clean'],
  ] as const)('%s runs only the supplied boxes', async (label, action) => {
    const user = userEvent.setup()
    render(<BoxRedoSection layers={['box', 'second']} />, { wrapper })
    await user.click(screen.getByRole('button', { name: label }))
    await waitFor(() =>
      expect(commands.redoOcrBoxes).toHaveBeenCalledWith(['box', 'second'], action, false),
    )
  })

  it('disables all redo buttons without a selection', () => {
    render(<BoxRedoSection layers={[]} />, { wrapper })
    for (const label of ['Read again', 'Translate again', 'Clean again'])
      expect(screen.getByRole('button', { name: label })).toBeDisabled()
  })

  it.each([
    ['Read again', 'read', { user_sources: 1, user_translations: 0 }],
    ['Translate again', 'translate', { user_sources: 0, user_translations: 1 }],
  ] as const)('confirms hand edits before %s', async (label, action, info) => {
    vi.spyOn(commands, 'getOcrRedoInfo').mockResolvedValue(info)
    const user = userEvent.setup()
    render(<BoxRedoSection layers={['box']} />, { wrapper })
    await user.click(screen.getByRole('button', { name: label }))
    await screen.findByRole('alertdialog')
    expect(commands.redoOcrBoxes).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(commands.redoOcrBoxes).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: label }))
    await user.click(await screen.findByRole('button', { name: 'Overwrite and run' }))
    await waitFor(() => expect(commands.redoOcrBoxes).toHaveBeenCalledWith(['box'], action, true))
  })

  it('shows SFX subtitle only for an SFX label and selects the new box', async () => {
    vi.spyOn(commands, 'getLineCategory').mockResolvedValue({
      value: 'sfx',
      source: 'user',
      confidence: null,
    })
    const user = userEvent.setup()
    render(<BoxRedoSection layers={['box']} />, { wrapper })
    await user.click(await screen.findByRole('button', { name: 'Add SFX subtitle' }))
    await waitFor(() => expect(commands.addSfxSubtitle).toHaveBeenCalledWith('box'))
    await waitFor(() => expect(useKoharuStore.getState().selectedLayers).toEqual(['subtitle']))
  })

  it('keeps a Process-menu overwrite dialog mounted after choosing its item', async () => {
    useKoharuStore.setState({ selectedLayers: ['box'] })
    vi.spyOn(commands, 'getOcrRedoInfo').mockResolvedValue({
      user_sources: 1,
      user_translations: 0,
    })
    const user = userEvent.setup()
    render(
      <Menubar>
        <MenubarMenu>
          <MenubarTrigger>Process</MenubarTrigger>
          <MenubarContent>
            <ProcessRedoItems />
          </MenubarContent>
        </MenubarMenu>
      </Menubar>,
      { wrapper },
    )
    await user.click(screen.getByRole('menuitem', { name: 'Process' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Read again' }))
    await user.click(await screen.findByRole('button', { name: 'Overwrite and run' }))
    await waitFor(() => expect(commands.redoOcrBoxes).toHaveBeenCalledWith(['box'], 'read', true))
  })
})

describe('OCR box preferences', () => {
  it('saves the default action without changing subtitle styling', async () => {
    const user = userEvent.setup()
    render(<OcrBoxPreferences />, { wrapper })
    await user.click(screen.getByRole('combobox', { name: 'OCR box default action' }))
    await user.click(await screen.findByRole('option', { name: 'Read only' }))
    await waitFor(() =>
      expect(commands.saveOcrBoxSettings).toHaveBeenCalledWith({
        ...settings,
        default_mode: 'read',
      }),
    )
  })

  it('saves subtitle size independently of pipeline settings and keeps the saved value on failure', async () => {
    const pipeline = vi.spyOn(commands, 'savePreferences')
    render(<OcrBoxPreferences />, { wrapper })
    const size = screen.getByRole('spinbutton', { name: 'Size in pixels' })
    fireEvent.change(size, { target: { value: '22' } })
    await waitFor(() =>
      expect(commands.saveOcrBoxSettings).toHaveBeenCalledWith({ ...settings, subtitle_size: 22 }),
    )
    await waitFor(() => expect(size).toBeEnabled())
    vi.spyOn(commands, 'saveOcrBoxSettings').mockRejectedValueOnce(new Error('Save failed'))
    fireEvent.change(size, { target: { value: '24' } })
    await waitFor(() => expect(commands.saveOcrBoxSettings).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(size).toHaveValue(22))
    expect(pipeline).not.toHaveBeenCalled()
  })
})
