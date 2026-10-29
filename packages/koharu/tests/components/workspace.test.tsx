import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { CanvasWorkspace, MIN_ZOOM, MAX_ZOOM } from '@/components/editor/CanvasWorkspace'
import { pageKey, pagesKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { emptyTextStyle } from '@/lib/text-styles'
import { commands, type Layer } from '@koharu/bridge/protocol'
import { toast } from '@koharu/ui/components/toast'
import { Slider } from '@koharu/ui/components/slider'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

const canvas = vi.hoisted(() => ({
  resize: vi.fn(),
  setView: vi.fn(),
  stageManifest: vi.fn(),
  installResource: vi.fn(),
  activateFrame: vi.fn(),
  activatePage: vi.fn(),
  clear: vi.fn(),
  previewOpacity: vi.fn(),
  beginTransform: vi.fn(),
  updateTransform: vi.fn(),
  finishTransform: vi.fn(),
  cancelTransform: vi.fn(),
  beginStroke: vi.fn(),
  extendStroke: vi.fn(),
  finishStroke: vi.fn(),
  cancelStroke: vi.fn(),
  sampleColor: vi.fn(),
  dispose: vi.fn(),
}))

const canvasState = vi.hoisted(() => ({
  canvas,
  error: null as Error | null,
  generation: 1 as number | null,
  hasFrame: true,
  retry: vi.fn(),
  status: 'ready' as 'loading' | 'switching' | 'ready' | 'recovering' | 'error',
}))
const prefetchCanvasPages = vi.hoisted(() => vi.fn(async () => []))

vi.mock('@/components/editor/useCanvas', () => ({
  useCanvas: () => canvasState,
}))
vi.mock('@koharu/bridge/canvas', () => ({
  prefetchCanvasPages,
  workspaceColor: () => [245, 245, 245],
}))

const layer: Extract<Layer, { type: 'image' }> = {
  type: 'image',
  id: 'element',
  parent: 'page',
  geometry: {
    points: [
      { x: 10, y: 20 },
      { x: 110, y: 20 },
      { x: 110, y: 70 },
      { x: 10, y: 70 },
    ],
  },
  visibility: { visible: true, opacity: 1 },
  image: 'image',
}

const paintLayer: Layer = {
  type: 'raster',
  id: 'paint',
  parent: 'page',
  visibility: { visible: true, opacity: 1 },
  image: 'paint-image',
  name: 'Paint 1',
  kind: 'paint',
}

let nextAnimationFrame = 1
let animationFrames = new Map<number, FrameRequestCallback>()

beforeEach(() => {
  nextAnimationFrame = 1
  animationFrames = new Map()
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
    const frame = nextAnimationFrame++
    animationFrames.set(frame, callback)
    return frame
  })
  vi.stubGlobal('cancelAnimationFrame', (frame: number) => animationFrames.delete(frame))
  canvasState.error = null
  canvasState.generation = 1
  canvasState.hasFrame = true
  canvasState.status = 'ready'
  prefetchCanvasPages.mockClear()
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

function installProject() {
  const page = {
    id: 'page',
    label: 'Page',
    size: { width: 1000, height: 1000 },
    layers: [layer],
    regions: [],
  }
  queryClient.setQueryData(projectKey, {
    name: 'Book',
    revision: 1,
    active_page: 'page',
    can_undo: false,
    can_redo: false,
  })
  queryClient.setQueryData(pagesKey, [])
  queryClient.setQueryData(pageKey, page)
  useKoharuStore.setState({ selectedLayers: [], tool: 'select' })
  useKoharuStore.setState({
    canvasPage: 'page',
    canvasRevision: 1,
    canvasGeneration: 1,
    canvasSize: [1000, 1000],
  })
}

function installNudgeProject() {
  installProject()

  queryClient.setQueryData(pageKey, (page: { layers: Layer[] }) => ({
    ...page,
    layers: [
      {
        type: 'text',
        id: 'element',
        parent: 'page',
        geometry: layer.geometry,
        angle_degrees: 0,
        visibility: { visible: true, opacity: 1 },
        content: {
          id: 'content',
          source: { text: 'Source', language: 'en' },
          translation: { text: 'Rendered', language: null },
          role: null,
          source_region: null,
        },
        typography: null,
        layout: 'paragraph',
        automatic_region: null,
      },
    ],
  }))

  useKoharuStore.setState({
    selectedLayers: ['element'],
    layerFrames: {
      element: { x: 30, y: 40, width: 50, height: 20, angle_degrees: 0 },
    },
  })
}

function renderWorkspace() {
  render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <CanvasWorkspace />
      </TooltipProvider>
    </QueryClientProvider>,
  )
  const surface = screen.getByLabelText('Koharu canvas')
  Object.defineProperty(surface, 'getBoundingClientRect', {
    value: () => ({ x: 10, y: 20, width: 800, height: 600 }),
  })
  return surface
}

describe('canvas interaction adapter', () => {
  it.each([
    ['1', { key: '1', code: 'Digit1' }],
    ['ctrl+shift+1', { key: '!', code: 'Digit1', ctrlKey: true, shiftKey: true }],
  ] as const)('applies preset %s once to both selected text boxes', async (binding, event) => {
    installNudgeProject()
    const page =
      queryClient.getQueryData<NonNullable<Awaited<ReturnType<typeof commands.getPage>>>>(pageKey)!
    const first = page.layers[0] as Extract<Layer, { type: 'text' }>
    const nextPage = {
      ...page,
      layers: [
        first,
        { ...first, id: 'second', content: { ...first.content, id: 'other-content' } },
      ],
    }
    queryClient.setQueryData(pageKey, nextPage)
    const style = {
      ...emptyTextStyle,
      fill: [255, 255, 255, 255] as [number, number, number, number],
    }
    useKoharuStore.setState({
      selectedLayers: ['element', 'second'],
      textStyles: {
        default_style: { ...emptyTextStyle },
        presets: [{ id: 'one', name: 'White', key: binding, style }],
      },
    })
    const apply = vi.spyOn(commands, 'applyTextStyle').mockResolvedValue({ changed: 2, skipped: 0, kept_manual: 0 })
    vi.spyOn(commands, 'getPage').mockResolvedValue(nextPage)
    vi.spyOn(commands, 'getPages').mockResolvedValue([])
    vi.spyOn(commands, 'getProject').mockResolvedValue(
      queryClient.getQueryData<Awaited<ReturnType<typeof commands.getProject>>>(projectKey)!,
    )
    renderWorkspace()
    fireEvent.keyDown(window, event)
    fireEvent.keyDown(window, { ...event, repeat: true })
    await waitFor(() =>
      expect(apply).toHaveBeenCalledExactlyOnceWith({ layers: ['element', 'second'] }, style),
    )
  })

  it('shows the usual error toast when a preset key fails', async () => {
    installNudgeProject()
    useKoharuStore.setState({
      selectedLayers: ['element'],
      textStyles: {
        default_style: { ...emptyTextStyle },
        presets: [{ id: 'one', name: 'White', key: '1', style: { ...emptyTextStyle } }],
      },
    })
    vi.spyOn(commands, 'applyTextStyle').mockRejectedValue(new Error('Style could not be applied'))
    const notify = vi.spyOn(toast, 'add')
    renderWorkspace()
    fireEvent.keyDown(window, { key: '1', code: 'Digit1' })
    await waitFor(() => expect(notify).toHaveBeenCalledWith(expect.objectContaining({
      type: 'error', description: 'Style could not be applied',
    })))
  })

  it('keeps preset digits in text entry and ignores composition, handled events and empty selections', async () => {
    installNudgeProject()
    useKoharuStore.setState({
      textStyles: {
        default_style: { ...emptyTextStyle },
        presets: [{ id: 'one', name: 'White', key: '1', style: { ...emptyTextStyle } }],
      },
    })
    const apply = vi.spyOn(commands, 'applyTextStyle').mockResolvedValue({ changed: 1, skipped: 0, kept_manual: 0 })
    renderWorkspace()
    const textarea = document.createElement('textarea')
    textarea.setAttribute('aria-label', 'Translation')
    document.body.append(textarea)
    await userEvent.setup().type(textarea, '1')
    expect(textarea).toHaveValue('1')
    fireEvent.keyDown(window, { key: '1', code: 'Digit1', isComposing: true })
    const handled = new KeyboardEvent('keydown', { key: '1', cancelable: true })
    handled.preventDefault()
    window.dispatchEvent(handled)
    useKoharuStore.setState({ selectedLayers: [] })
    fireEvent.keyDown(window, { key: '1', code: 'Digit1' })
    expect(apply).not.toHaveBeenCalled()
    textarea.remove()
  })

  it('prefetches only after the authoritative canvas page and generation are active', async () => {
    installProject()
    queryClient.setQueryData(pagesKey, [
      {
        id: 'page',
        label: 'Page',
        size: { width: 1000, height: 1000 },
        source_asset: null,
        layer_count: 1,
      },
      {
        id: 'next',
        label: 'Next',
        size: { width: 1000, height: 1000 },
        source_asset: null,
        layer_count: 1,
      },
    ])
    useKoharuStore.setState({ canvasPage: 'previous' })
    renderWorkspace()

    await Promise.resolve()
    expect(prefetchCanvasPages).not.toHaveBeenCalled()

    act(() => useKoharuStore.setState({ canvasPage: 'page' }))
    await waitFor(() => expect(prefetchCanvasPages).toHaveBeenCalledWith(['next']))
  })

  it('renders an accessible browser canvas and keeps camera updates local', async () => {
    installProject()
    const surface = renderWorkspace()
    expect(screen.getByTestId('webgpu-canvas')).toBeInstanceOf(HTMLCanvasElement)

    fireEvent.wheel(surface, { clientX: 100, clientY: 100, deltaY: 4 })
    fireEvent.wheel(surface, { clientX: 100, clientY: 100, deltaY: 6 })

    await waitFor(() => expect(canvas.setView).toHaveBeenLastCalledWith(1, [0, -10]))
  })

  it('preserves a manual camera when a newer canvas generation becomes ready', () => {
    installProject()
    renderWorkspace()
    const camera = { zoom: 2, translation: [-120, -80] as [number, number], fitted: false }

    act(() => useKoharuStore.setState({ camera }))
    act(() => {
      canvasState.status = 'switching'
      useKoharuStore.setState({ canvasGeneration: 2 })
    })
    act(() => {
      canvasState.status = 'ready'
      canvasState.generation = 2
      useKoharuStore.setState({ canvasRevision: 2 })
    })

    expect(useKoharuStore.getState().camera).toEqual(camera)
  })

  it('keeps wheel zoom anchored at the pointer and bounded by the shared limits', () => {
    installProject()
    const surface = renderWorkspace()
    act(() => useKoharuStore.setState({ camera: { zoom: 1, translation: [0, 0], fitted: false } }))
    fireEvent.wheel(surface, { clientX: 210, clientY: 220, deltaY: -100, ctrlKey: true })
    const camera = useKoharuStore.getState().camera
    expect((200 - camera.translation[0]) / camera.zoom).toBeCloseTo(200)
    expect((200 - camera.translation[1]) / camera.zoom).toBeCloseTo(200)
    fireEvent.wheel(surface, { clientX: 210, clientY: 220, deltaY: -100000, ctrlKey: true })
    expect(useKoharuStore.getState().camera.zoom).toBe(MAX_ZOOM)
    expect(screen.getByText('1600%')).toBeInTheDocument()
    fireEvent.wheel(surface, { clientX: 210, clientY: 220, deltaY: 100000, ctrlKey: true })
    expect(useKoharuStore.getState().camera.zoom).toBe(MIN_ZOOM)
    expect(screen.getByText('2%')).toBeInTheDocument()
  })

  it('clamps translation when the status-bar slider changes zoom', async () => {
    installProject()
    renderWorkspace()
    act(() =>
      useKoharuStore.setState({ camera: { zoom: 1, translation: [1000, 1000], fitted: false } }),
    )
    const slider = document.querySelector<HTMLInputElement>('footer input[type="range"]')!
    expect(slider).not.toBeNull()
    fireEvent.keyDown(slider, { key: 'ArrowRight' })
    expect(useKoharuStore.getState().camera).toEqual({
      zoom: 1.05,
      translation: [700, 500],
      fitted: false,
    })
    expect(screen.getByText('105%')).toBeInTheDocument()
  })

  it.each([
    [{ key: '-', code: 'Minus', ctrlKey: true }, 1 / 1.2, '83%'],
    [{ key: '=', code: 'Equal', ctrlKey: true }, 1.2, '120%'],
    [{ key: '+', code: 'Equal', ctrlKey: true, shiftKey: true }, 1.2, '120%'],
    [{ key: '+', code: 'NumpadAdd', ctrlKey: true }, 1.2, '120%'],
    [{ key: '-', code: 'NumpadSubtract', ctrlKey: true }, 1 / 1.2, '83%'],
    [{ key: '=', code: 'Equal', metaKey: true }, 1.2, '120%'],
  ] as const)('zooms the camera and status label with %j', (options, expected, label) => {
    installProject()
    renderWorkspace()
    const undo = vi.spyOn(commands, 'undo').mockResolvedValue(null)
    const commit = vi.spyOn(commands, 'commitTransform').mockResolvedValue(null)
    const typography = vi.spyOn(commands, 'setTypography').mockResolvedValue(null)
    act(() => useKoharuStore.setState({ camera: { zoom: 1, translation: [0, 0], fitted: false } }))
    const event = new KeyboardEvent('keydown', { ...options, bubbles: true, cancelable: true })
    fireEvent(window, event)
    const camera = useKoharuStore.getState().camera
    expect(camera.zoom).toBeCloseTo(expected)
    expect((400 - camera.translation[0]) / camera.zoom).toBeCloseTo(400)
    expect((300 - camera.translation[1]) / camera.zoom).toBeCloseTo(300)
    expect(screen.getByText(label)).toBeInTheDocument()
    expect(event.defaultPrevented).toBe(true)
    expect(undo).not.toHaveBeenCalled()
    expect(commit).not.toHaveBeenCalled()
    expect(typography).not.toHaveBeenCalled()
  })

  it('snaps through 100 percent, resets to actual size, keeps Fit and enforces zoom limits', () => {
    installProject()
    const surface = renderWorkspace()
    act(() =>
      useKoharuStore.setState({ camera: { zoom: 0.9, translation: [0, 0], fitted: false } }),
    )
    fireEvent.keyDown(window, { key: '=', ctrlKey: true })
    expect(useKoharuStore.getState().camera.zoom).toBe(1)
    act(() =>
      useKoharuStore.setState({ camera: { zoom: 1.1, translation: [0, 0], fitted: false } }),
    )
    fireEvent.keyDown(window, { key: '-', ctrlKey: true })
    expect(useKoharuStore.getState().camera.zoom).toBe(1)
    fireEvent.keyDown(window, { key: '=', ctrlKey: true })
    fireEvent.keyDown(window, { key: '0', ctrlKey: true })
    expect(screen.getByText('100%')).toBeInTheDocument()
    fireEvent.keyDown(window, { key: '0' })
    expect(useKoharuStore.getState().camera.fitted).toBe(true)
    act(() =>
      useKoharuStore.setState({ camera: { zoom: MAX_ZOOM, translation: [0, 0], fitted: false } }),
    )
    fireEvent.keyDown(window, { key: '=', ctrlKey: true })
    expect(useKoharuStore.getState().camera.zoom).toBe(MAX_ZOOM)
    act(() =>
      useKoharuStore.setState({ camera: { zoom: MIN_ZOOM, translation: [0, 0], fitted: false } }),
    )
    fireEvent.keyDown(window, { key: '-', ctrlKey: true })
    expect(useKoharuStore.getState().camera.zoom).toBe(MIN_ZOOM)
    expect(surface).toBeInTheDocument()
  })

  it('throttles held zoom keys and ignores repeats for actual size', () => {
    vi.useFakeTimers()
    installProject()
    renderWorkspace()
    fireEvent.keyDown(window, { key: '=', ctrlKey: true })
    fireEvent.keyDown(window, { key: '=', ctrlKey: true, repeat: true })
    act(() => vi.advanceTimersByTime(99))
    fireEvent.keyDown(window, { key: '=', ctrlKey: true, repeat: true })
    expect(useKoharuStore.getState().camera.zoom).toBeCloseTo(1.2)
    act(() => vi.advanceTimersByTime(1))
    fireEvent.keyDown(window, { key: '=', ctrlKey: true, repeat: true })
    expect(useKoharuStore.getState().camera.zoom).toBeCloseTo(1.44)
    fireEvent.keyDown(window, { key: '0', ctrlKey: true, repeat: true })
    expect(useKoharuStore.getState().camera.zoom).toBeCloseTo(1.44)
  })

  it('handles modified zoom after slider input and host prevention, but protects text entry and composition', () => {
    installProject()
    renderWorkspace()
    const slider = document.querySelector<HTMLInputElement>('footer input[type="range"]')!
    slider.focus()
    const event = new KeyboardEvent('keydown', {
      key: '-',
      ctrlKey: true,
      bubbles: true,
      cancelable: true,
    })
    event.preventDefault()
    fireEvent(slider, event)
    const zoom = useKoharuStore.getState().camera.zoom
    expect(zoom).toBeCloseTo(1 / 1.2)
    for (const control of [document.createElement('input'), document.createElement('textarea')]) {
      document.body.append(control)
      const typed = new KeyboardEvent('keydown', {
        key: '-',
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      })
      fireEvent(control, typed)
      expect(typed.defaultPrevented).toBe(true)
      expect(useKoharuStore.getState().camera.zoom).toBe(zoom)
      control.remove()
    }
    const composing = new KeyboardEvent('keydown', {
      key: '-',
      ctrlKey: true,
      isComposing: true,
      cancelable: true,
    })
    fireEvent(window, composing)
    expect(composing.defaultPrevented).toBe(true)
    expect(useKoharuStore.getState().camera.zoom).toBe(zoom)
  })

  it('uses changed zoom modifiers and keys, and keeps bare bindings behind the handled-key guard', () => {
    installProject()
    renderWorkspace()
    useKoharuStore.getState().setShortcut('zoom_in', 'alt+z')
    fireEvent.keyDown(window, { key: '=', ctrlKey: true })
    expect(useKoharuStore.getState().camera.zoom).toBe(1)
    fireEvent.keyDown(window, { key: 'z', altKey: true })
    expect(useKoharuStore.getState().camera.zoom).toBe(1.2)
    useKoharuStore.getState().setShortcut('zoom_in', 'shift+]')
    fireEvent.keyDown(window, { key: '}', code: 'BracketRight', shiftKey: true })
    expect(useKoharuStore.getState().camera.zoom).toBeCloseTo(1.44)
    useKoharuStore.getState().setShortcut('zoom_in', '=')
    const handled = new KeyboardEvent('keydown', { key: '=', cancelable: true })
    handled.preventDefault()
    fireEvent(window, handled)
    expect(useKoharuStore.getState().camera.zoom).toBeCloseTo(1.44)
    fireEvent.keyDown(window, { key: '=' })
    expect(useKoharuStore.getState().camera.zoom).toBeCloseTo(1.728)
  })

  it('does not zoom without an open page', () => {
    queryClient.setQueryData(pageKey, null)
    renderWorkspace()
    fireEvent.keyDown(window, { key: '=', ctrlKey: true })
    expect(useKoharuStore.getState().camera.zoom).toBe(1)
  })

  it.each(['ctrl', 'wheel'] as const)(
    'keeps wheel gestures and the pointer anchor consistent in %s mode',
    (wheelZoom) => {
      installProject()
      useKoharuStore.getState().setWheelZoom(wheelZoom)
      const surface = renderWorkspace()
      const reset = () =>
        act(() =>
          useKoharuStore.setState({ camera: { zoom: 1, translation: [0, 0], fitted: false } }),
        )
      for (const ctrlKey of [false, true]) {
        reset()
        fireEvent.wheel(surface, { clientX: 210, clientY: 220, deltaY: -100, ctrlKey })
        const camera = useKoharuStore.getState().camera
        if (ctrlKey === (wheelZoom === 'ctrl')) {
          expect(camera.zoom).toBeGreaterThan(1)
          expect((200 - camera.translation[0]) / camera.zoom).toBeCloseTo(200)
          expect((200 - camera.translation[1]) / camera.zoom).toBeCloseTo(200)
        } else expect(camera).toEqual({ zoom: 1, translation: [0, 100], fitted: false })
      }
      reset()
      fireEvent.wheel(surface, { deltaY: 20, shiftKey: true })
      expect(useKoharuStore.getState().camera).toEqual({
        zoom: 1,
        translation: [-20, 0],
        fitted: false,
      })
      reset()
      act(() => useKoharuStore.setState({ tool: 'draw' }))
      fireEvent.wheel(surface, { deltaY: 20, altKey: true })
      expect(useKoharuStore.getState().brush.diameter).toBe(44)
      expect(useKoharuStore.getState().camera).toEqual({
        zoom: 1,
        translation: [0, 0],
        fitted: false,
      })
    },
  )

  it('treats an indistinguishable Ctrl+wheel pinch as scrolling in wheel-alone mode', () => {
    installProject()
    useKoharuStore.getState().setWheelZoom('wheel')
    const surface = renderWorkspace()
    fireEvent.wheel(surface, { deltaY: 0.5, ctrlKey: true })
    expect(useKoharuStore.getState().camera.zoom).toBe(1)
    expect(useKoharuStore.getState().camera.translation[1]).toBeCloseTo(-0.5)
  })

  it('suppresses Alt browser handling while an editor input retains focus', () => {
    installProject()
    renderWorkspace()
    const input = document.createElement('input')
    document.body.append(input)
    input.focus()

    const event = new KeyboardEvent('keydown', { key: 'Alt', bubbles: true, cancelable: true })
    input.dispatchEvent(event)

    expect(event.defaultPrevented).toBe(true)
    input.remove()
  })

  it('allows bare tool shortcuts from a slider while leaving control keys alone', () => {
    installProject()
    renderWorkspace()
    const input = document.createElement('input')
    input.type = 'range'
    document.body.append(input)
    input.focus()
    const undo = vi.spyOn(commands, 'undo').mockResolvedValue(null)
    const remove = vi.spyOn(commands, 'deleteLayers').mockResolvedValue(null)
    act(() => useKoharuStore.setState({ selectedLayers: ['element'] }))

    for (const options of [
      { key: ' ', code: 'Space' },
      { key: 'ArrowLeft' },
      { key: 'Delete' },
      { key: 'z', ctrlKey: true },
    ]) {
      const event = new KeyboardEvent('keydown', { ...options, bubbles: true, cancelable: true })
      fireEvent(input, event)
      expect(event.defaultPrevented).toBe(false)
    }
    expect(undo).not.toHaveBeenCalled()
    expect(remove).not.toHaveBeenCalled()
    expect(useKoharuStore.getState().selectedLayers).toEqual(['element'])

    fireEvent.keyDown(input, { key: 't' })
    expect(useKoharuStore.getState().tool).toBe('text')
    input.remove()
  })

  it('keeps text entry, composition and handled keys out of tool shortcuts', () => {
    installProject()
    renderWorkspace()
    for (const tag of ['input', 'textarea', 'select']) {
      const control = document.createElement(tag)
      document.body.append(control)
      fireEvent.keyDown(control, { key: 't' })
      expect(useKoharuStore.getState().tool).toBe('select')
      control.remove()
    }
    fireEvent.keyDown(window, { key: 't', isComposing: true })
    expect(useKoharuStore.getState().tool).toBe('select')
    const handled = new KeyboardEvent('keydown', { key: 't', cancelable: true })
    handled.preventDefault()
    fireEvent(window, handled)
    expect(useKoharuStore.getState().tool).toBe('select')
  })

  it('matches modified and alternate tool bindings from a focused slider', () => {
    installProject()
    renderWorkspace()
    const slider = document.createElement('input')
    slider.type = 'range'
    document.body.append(slider)
    useKoharuStore.getState().setShortcut('text', 'alt+z')
    useKoharuStore.getState().setShortcut('text_alt', 'f2')
    fireEvent.keyDown(slider, { key: 'z', altKey: true })
    expect(useKoharuStore.getState().tool).toBe('text')
    act(() => useKoharuStore.getState().setTool('select'))
    fireEvent.keyDown(window, { key: 'F2' })
    expect(useKoharuStore.getState().tool).toBe('text')
    slider.remove()
  })

  it('announces WebGPU startup failures and offers recovery', () => {
    installProject()
    canvasState.status = 'error'
    canvasState.error = new Error('No compatible WebGPU adapter was found.')
    renderWorkspace()
    expect(screen.getByRole('alert')).toHaveTextContent('WebGPU canvas unavailable')
    expect(screen.getByRole('alert')).toHaveTextContent('No compatible WebGPU adapter was found.')
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }))
    expect(canvasState.retry).toHaveBeenCalledOnce()
  })

  it('previews brush input locally and sends only the durable paint commit to Rust', async () => {
    installProject()
    useKoharuStore.setState({ tool: 'draw', brush: { diameter: 48, color: '#FFFFFF' } })
    const commit = vi
      .spyOn(commands, 'commitPaint')
      .mockResolvedValue({ revision: 2, layer: 'paint' })
    const surface = renderWorkspace()
    expect(surface).toHaveStyle({ cursor: 'none' })

    fireEvent.pointerDown(surface, { button: 0, pointerId: 7, clientX: 30, clientY: 40 })
    fireEvent.pointerMove(surface, { pointerId: 7, clientX: 55, clientY: 65 })
    fireEvent.pointerUp(surface, { pointerId: 7, clientX: 58, clientY: 70 })

    await waitFor(() => expect(commit).toHaveBeenCalledOnce())
    expect(canvas.beginStroke).toHaveBeenCalledWith({
      kind: 'paint',
      layer: null,
      point: { x: 20, y: 20 },
      diameter: 48,
      color: [255, 255, 255, 255],
    })
    expect(canvas.extendStroke).toHaveBeenCalledWith(expect.arrayContaining([{ x: 45, y: 45 }]))
    expect(canvas.finishStroke).toHaveBeenCalledOnce()
    expect(commit).toHaveBeenCalledWith(1, null, expect.arrayContaining([{ x: 45, y: 45 }]), {
      diameter: 48,
      color: [255, 255, 255, 255],
    })
  })
  it.each([
    {
      keys: ['ArrowRight'],
      repeat: 'ArrowRight',
      release: 'ArrowRight',
      expected: { x: 32, y: 40 },
    },
    {
      keys: ['ArrowUp', 'ArrowRight'],
      repeat: 'ArrowRight',
      release: 'ArrowRight',
      expected: { x: 32, y: 36 },
    },
    {
      keys: ['ArrowRight', 'ArrowUp'],
      repeat: 'ArrowUp',
      release: 'ArrowRight',
      expected: { x: 33, y: 37 },
    },
    {
      keys: ['ArrowLeft', 'ArrowRight'],
      repeat: 'ArrowRight',
      release: 'ArrowRight',
      expected: { x: 28, y: 40 },
    },
    {
      keys: ['ArrowUp', 'ArrowDown'],
      repeat: 'ArrowDown',
      release: 'ArrowDown',
      expected: { x: 30, y: 38 },
    },
  ])('text box nudge with arrow key: $keys', async ({ keys, repeat, release, expected }) => {
    vi.useFakeTimers()
    installNudgeProject()

    const commit = vi.spyOn(commands, 'commitTransform').mockResolvedValue(2)
    const surface = renderWorkspace()

    act(() =>
      useKoharuStore.setState({
        camera: { zoom: 4, translation: [0, 0], fitted: false },
      }),
    )

    for (const key of keys) fireEvent.keyDown(surface, { key })

    act(() => {
      vi.advanceTimersByTime(300)
    })

    fireEvent.keyDown(surface, { key: repeat, repeat: true })

    fireEvent.keyUp(surface, { key: release })
    const remaining = keys.filter((key) => key !== release)

    if (remaining.length) {
      expect(commit).not.toHaveBeenCalled()
    }

    act(() => {
      vi.advanceTimersByTime(30)
    })

    for (const key of remaining) {
      fireEvent.keyUp(surface, { key })
    }

    vi.useRealTimers()

    await waitFor(() =>
      expect(commit).toHaveBeenCalledWith(
        1,
        [
          {
            element: 'element',
            frame: {
              ...expected,
              width: 50,
              height: 20,
              angle_degrees: 0,
            },
          },
        ],
        false,
      ),
    )
  })
  it('commits one continuous arrow hold once and blocks pointer gestures during it', async () => {
    vi.useFakeTimers()
    installNudgeProject()
    const commit = vi.spyOn(commands, 'commitTransform').mockResolvedValue(2)
    const surface = renderWorkspace()

    fireEvent.keyDown(surface, { key: 'ArrowRight' })
    act(() => vi.advanceTimersByTime(1000))
    fireEvent.pointerDown(surface, { button: 0, pointerId: 7, clientX: 200, clientY: 200 })
    expect(useKoharuStore.getState().selectedLayers).toEqual(['element'])
    expect(canvas.beginTransform).toHaveBeenCalledOnce()
    expect(commit).not.toHaveBeenCalled()

    fireEvent.keyUp(surface, { key: 'ArrowRight' })
    vi.useRealTimers()
    await waitFor(() =>
      expect(commit).toHaveBeenCalledExactlyOnceWith(
        1,
        [{ element: 'element', frame: { x: 55, y: 40, width: 50, height: 20, angle_degrees: 0 } }],
        false,
      ),
    )
  })

  it('nudges ten pixels with Shift and returns to one pixel when Shift is released', async () => {
    vi.useFakeTimers()
    installNudgeProject()
    const commit = vi.spyOn(commands, 'commitTransform').mockResolvedValue(2)
    const surface = renderWorkspace()

    fireEvent.keyDown(surface, { key: 'ArrowRight', shiftKey: true })
    act(() => vi.advanceTimersByTime(300))
    fireEvent.keyUp(surface, { key: 'Shift' })
    act(() => vi.advanceTimersByTime(30))
    fireEvent.keyUp(surface, { key: 'ArrowRight' })
    vi.useRealTimers()

    await waitFor(() =>
      expect(commit).toHaveBeenCalledExactlyOnceWith(
        1,
        [{ element: 'element', frame: { x: 51, y: 40, width: 50, height: 20, angle_degrees: 0 } }],
        false,
      ),
    )
  })

  it('commits separate arrow taps as separate undo steps', async () => {
    installNudgeProject()
    const commit = vi
      .spyOn(commands, 'commitTransform')
      .mockImplementation(async (revision, frames) => {
        useKoharuStore.setState({
          canvasRevision: revision + 1,
          layerFrames: Object.fromEntries(frames.map(({ element, frame }) => [element, frame])),
        })
        return revision + 1
      })
    const surface = renderWorkspace()

    for (let index = 0; index < 4; index++) {
      await act(async () => {
        fireEvent.keyDown(surface, { key: 'ArrowRight' })
        fireEvent.keyUp(surface, { key: 'ArrowRight' })
      })
      expect(commit).toHaveBeenNthCalledWith(
        index + 1,
        index + 1,
        [
          {
            element: 'element',
            frame: { x: 31 + index, y: 40, width: 50, height: 20, angle_degrees: 0 },
          },
        ],
        false,
      )
    }
    expect(commit).toHaveBeenCalledTimes(4)
  })

  it('leaves arrow keys to a focused slider without nudging selected text', async () => {
    installNudgeProject()
    const commit = vi.spyOn(commands, 'commitTransform').mockResolvedValue(2)
    renderWorkspace()
    render(<Slider defaultValue={50} />)
    const sliders = await screen.findAllByRole('slider', { hidden: true })
    const slider = sliders.at(-1)!
    act(() => slider.focus())

    fireEvent.keyDown(slider, { key: 'ArrowRight' })
    fireEvent.keyUp(slider, { key: 'ArrowRight' })

    expect(slider).toHaveAttribute('aria-valuenow', '51')
    expect(canvas.beginTransform).not.toHaveBeenCalled()
    expect(commit).not.toHaveBeenCalled()
  })

  it.each(['Escape', 'blur', 'pointercancel'])('stops a held nudge on %s', async (reason) => {
    vi.useFakeTimers()
    installNudgeProject()
    const commit = vi.spyOn(commands, 'commitTransform').mockResolvedValue(2)
    const surface = renderWorkspace()
    fireEvent.keyDown(surface, { key: 'ArrowRight' })
    if (reason === 'Escape') fireEvent.keyDown(surface, { key: 'Escape' })
    else if (reason === 'blur') fireEvent(window, new Event('blur'))
    else fireEvent.pointerCancel(surface)
    act(() => vi.advanceTimersByTime(1000))
    fireEvent.keyUp(surface, { key: 'ArrowRight' })
    vi.useRealTimers()

    if (reason === 'blur') await waitFor(() => expect(commit).toHaveBeenCalledOnce())
    else {
      expect(canvas.cancelTransform).toHaveBeenCalledOnce()
      expect(commit).not.toHaveBeenCalled()
    }
    expect(canvas.updateTransform).toHaveBeenCalledTimes(reason === 'blur' ? 1 : 0)
  })

  it.each([
    ['revision', { canvasRevision: 2 }],
    ['generation', { canvasGeneration: 2 }],
  ])('cancels an active gesture when the canvas %s changes', async (_name, update) => {
    installProject()
    useKoharuStore.setState({ tool: 'draw', brush: { diameter: 48, color: '#FFFFFF' } })
    const commit = vi
      .spyOn(commands, 'commitPaint')
      .mockResolvedValue({ revision: 2, layer: 'paint' })
    const surface = renderWorkspace()

    fireEvent.pointerDown(surface, { button: 0, pointerId: 8, clientX: 30, clientY: 40 })
    expect(canvas.beginStroke).toHaveBeenCalledOnce()

    act(() => {
      useKoharuStore.setState(update)
    })

    await waitFor(() => expect(canvas.cancelStroke).toHaveBeenCalledOnce())
    fireEvent.pointerUp(surface, { pointerId: 8, clientX: 30, clientY: 40 })
    expect(canvas.finishStroke).not.toHaveBeenCalled()
    expect(commit).not.toHaveBeenCalled()
  })

  it('uses rendered text bounds for hit testing and semantic transforms', async () => {
    installProject()
    queryClient.setQueryData(pageKey, (page: { layers: Layer[] }) => ({
      ...page,
      layers: [
        {
          type: 'text',
          id: 'element',
          parent: 'page',
          geometry: layer.geometry,
          angle_degrees: 0,
          visibility: { visible: true, opacity: 1 },
          content: {
            id: 'content',
            source: { text: 'Source', language: 'en' },
            translation: { text: 'Rendered', language: null },
            role: null,
            source_region: null,
          },
          typography: null,
          layout: 'paragraph',
          automatic_region: null,
        },
      ],
    }))
    useKoharuStore.setState({
      layerFrames: {
        element: { x: 30, y: 40, width: 50, height: 20, angle_degrees: 0 },
      },
    })
    const commit = vi.spyOn(commands, 'commitTransform').mockResolvedValue(2)
    const surface = renderWorkspace()

    fireEvent.pointerDown(surface, { button: 0, pointerId: 9, clientX: 50, clientY: 60 })
    fireEvent.pointerMove(surface, { pointerId: 9, clientX: 70, clientY: 80 })
    fireEvent.pointerUp(surface, { pointerId: 9, clientX: 70, clientY: 80 })

    await waitFor(() => expect(commit).toHaveBeenCalledOnce())
    expect(useKoharuStore.getState().selectedLayers).toEqual(['element'])
    expect(canvas.beginTransform).toHaveBeenCalledWith([
      {
        element: 'element',
        frame: { x: 30, y: 40, width: 50, height: 20, angle_degrees: 0 },
      },
    ])
    expect(canvas.updateTransform).toHaveBeenCalledWith(
      expect.arrayContaining([
        expect.objectContaining({
          element: 'element',
          frame: expect.objectContaining({ x: 50, y: 60 }),
        }),
      ]),
    )
    expect(canvas.finishTransform).toHaveBeenCalledOnce()
    expect(commit).toHaveBeenCalledWith(
      1,
      expect.arrayContaining([
        expect.objectContaining({
          element: 'element',
          frame: expect.objectContaining({ x: 50, y: 60 }),
        }),
      ]),
      false,
    )
  })

  it('resizes a selected layer through Koharu selection controls', async () => {
    installProject()
    useKoharuStore.setState({ selectedLayers: ['element'] })
    const commit = vi.spyOn(commands, 'commitTransform').mockResolvedValue(2)
    renderWorkspace()
    Object.defineProperty(screen.getByTestId('canvas-overlay'), 'getBoundingClientRect', {
      value: () => ({ x: 10, y: 20, width: 800, height: 600 }),
    })
    const handle = document.querySelector<HTMLElement>('[data-resize-handle="e"]')!

    fireEvent.pointerDown(handle, { button: 0, pointerId: 10, clientX: 120, clientY: 65 })
    fireEvent.pointerMove(handle, { pointerId: 10, clientX: 140, clientY: 65 })
    fireEvent.pointerUp(handle, { pointerId: 10, clientX: 140, clientY: 65 })

    await waitFor(() => expect(commit).toHaveBeenCalledOnce())
    expect(canvas.beginTransform).toHaveBeenCalledWith([
      { element: 'element', frame: { x: 10, y: 20, width: 100, height: 50, angle_degrees: 0 } },
    ])
    expect(canvas.updateTransform).toHaveBeenCalledWith(
      expect.arrayContaining([
        {
          element: 'element',
          frame: { x: 10, y: 20, width: 120, height: 50, angle_degrees: 0 },
        },
      ]),
    )
    expect(commit).toHaveBeenCalledWith(
      1,
      expect.arrayContaining([
        {
          element: 'element',
          frame: { x: 10, y: 20, width: 120, height: 50, angle_degrees: 0 },
        },
      ]),
      false,
    )
  })

  it('shows the automatic region behind the selected text controls', () => {
    installProject()
    queryClient.setQueryData(pageKey, (page: { layers: Layer[] }) => ({
      ...page,
      layers: [
        {
          type: 'text',
          id: 'element',
          parent: 'page',
          geometry: null,
          angle_degrees: null,
          visibility: { visible: true, opacity: 1 },
          content: {
            id: 'content',
            source: { text: 'Source', language: 'en' },
            translation: { text: 'Rendered', language: null },
            role: null,
            source_region: null,
          },
          typography: null,
          layout: 'paragraph',
          automatic_region: 'bubble',
        },
      ],
      regions: [
        {
          id: 'bubble',
          parent: 'page',
          geometry: {
            points: [
              { x: 20, y: 30 },
              { x: 100, y: 30 },
              { x: 100, y: 90 },
              { x: 20, y: 90 },
            ],
          },
          kind: 'bubble',
          label: null,
        },
      ],
    }))
    useKoharuStore.setState({
      selectedLayers: ['element'],
      layerFrames: {
        element: { x: 30, y: 40, width: 50, height: 20, angle_degrees: 0 },
      },
    })

    renderWorkspace()

    expect(screen.getByTestId('text-fit-region').querySelector('polygon')).toHaveAttribute(
      'points',
      '20,30 100,30 100,90 20,90',
    )
  })

  it('targets the selected raster layer with the eraser', async () => {
    installProject()
    queryClient.setQueryData(pageKey, (page: { layers: Layer[] }) => ({
      ...page,
      layers: [...page.layers, paintLayer],
    }))
    useKoharuStore.setState({ tool: 'eraser', selectedLayers: ['paint'] })
    const commit = vi
      .spyOn(commands, 'commitErase')
      .mockResolvedValue({ revision: 2, layer: 'paint' })
    const surface = renderWorkspace()

    fireEvent.pointerDown(surface, { button: 0, pointerId: 11, clientX: 30, clientY: 40 })
    fireEvent.pointerUp(surface, { pointerId: 11, clientX: 30, clientY: 40 })

    await waitFor(() => expect(commit).toHaveBeenCalledOnce())
    expect(canvas.beginStroke).toHaveBeenCalledWith({
      kind: 'erase',
      layer: 'paint',
      point: { x: 20, y: 20 },
      diameter: 48,
    })
    expect(commit).toHaveBeenCalledWith(1, 'paint', expect.arrayContaining([{ x: 20, y: 20 }]), 48)
  })

  it('maps the Remove tool to an inpainting mask gesture', async () => {
    installProject()
    useKoharuStore.setState({ tool: 'remove' })
    const commit = vi.spyOn(commands, 'commitInpaint').mockResolvedValue('job')
    const surface = renderWorkspace()

    fireEvent.pointerDown(surface, { button: 0, pointerId: 12, clientX: 30, clientY: 40 })
    fireEvent.pointerUp(surface, { pointerId: 12, clientX: 30, clientY: 40 })

    await waitFor(() => expect(commit).toHaveBeenCalledOnce())
    expect(canvas.beginStroke).toHaveBeenCalledWith({
      kind: 'inpaint',
      layer: null,
      point: { x: 20, y: 20 },
      diameter: 48,
    })
    expect(commit).toHaveBeenCalledWith(1, expect.arrayContaining([{ x: 20, y: 20 }]), 48)
  })

  it('creates point text on click and paragraph text on drag', async () => {
    installProject()
    useKoharuStore.setState({ tool: 'text' })
    const point = vi
      .spyOn(commands, 'addPointText')
      .mockResolvedValue({ revision: 2, layer: 'point-text' })
    const box = vi
      .spyOn(commands, 'addTextBox')
      .mockResolvedValue({ revision: 3, layer: 'box-text' })
    const surface = renderWorkspace()

    fireEvent.pointerDown(surface, { button: 0, pointerId: 13, clientX: 30, clientY: 40 })
    fireEvent.pointerUp(surface, { pointerId: 13, clientX: 30, clientY: 40 })
    await waitFor(() => expect(point).toHaveBeenCalledWith({ x: 20, y: 20 }))

    fireEvent.pointerDown(surface, { button: 0, pointerId: 14, clientX: 40, clientY: 50 })
    fireEvent.pointerMove(surface, { pointerId: 14, clientX: 140, clientY: 110 })
    fireEvent.pointerUp(surface, { pointerId: 14, clientX: 140, clientY: 110 })
    await waitFor(() => expect(box).toHaveBeenCalledOnce())
    expect(box).toHaveBeenCalledWith({
      x: 30,
      y: 30,
      width: 100,
      height: 60,
      angle_degrees: 0,
    })
  })
})
