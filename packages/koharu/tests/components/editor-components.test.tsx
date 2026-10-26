import { QueryClientProvider } from '@tanstack/react-query'
import {
  act,
  fireEvent,
  render as testingRender,
  renderHook,
  screen,
  waitFor,
} from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ThemeProvider } from 'next-themes'
import { StrictMode, type ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

import { TitleBar } from '@/components/app/TitleBar'
import { WindowControls } from '@/components/app/WindowChrome'
import { FontPicker } from '@/components/controls/FontPicker'
import { ActivityCenter } from '@/components/editor/ActivityCenter'
import { CanvasCommandBar } from '@/components/editor/CanvasCommandBar'
import { Inspector } from '@/components/editor/Inspector'
import { PageRail } from '@/components/editor/PageRail'
import { ResourceMonitor } from '@/components/editor/ResourceMonitor'
import { StatusBar } from '@/components/editor/StatusBar'
import { ToolBar } from '@/components/editor/ToolBar'
import { ProviderPreferences } from '@/components/preferences/ProviderPreferences'
import { SettingsPage } from '@/components/preferences/SettingsPage'
import {
  fontsKey,
  pageKey,
  pagesKey,
  preparedPageKey,
  projectKey,
  queryClient,
  useCommand,
} from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import * as canvasRuntime from '@koharu/bridge/canvas'
import {
  commands,
  type FontFamily,
  type Layer,
  type PageSummary,
  type Preferences,
  type ProjectInfo,
} from '@koharu/bridge/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'
import { toast } from '@koharu/ui/components/toast'

const nativeWindow = vi.hoisted(() => ({
  close: vi.fn(async () => undefined),
  isMaximized: vi.fn(async () => false),
  minimize: vi.fn(async () => undefined),
  onResized: vi.fn(async () => () => undefined),
  startResizeDragging: vi.fn(async () => undefined),
  toggleMaximize: vi.fn(async () => undefined),
}))
const nativeOpenUrl = vi.hoisted(() => vi.fn(async () => undefined))
const nativeGetVersion = vi.hoisted(() => vi.fn(async () => '0.62.0'))

vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => nativeWindow }))
vi.mock('@tauri-apps/api/app', () => ({ getVersion: nativeGetVersion }))
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: nativeOpenUrl }))

const emptyCredential = () => ({ configured: false, value: null, clear: false })

const textLayer: Layer = {
  type: 'text',
  angle_degrees: 0,
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
  content: {
    id: 'content',
    source: { text: 'こんにちは', language: 'ja' },
    translation: { text: 'Hello', language: null },
    role: null,
    source_region: null,
  },
  typography: {
    preferred_font: 'Noto Sans',
    font_weight: 400,
    font_style: 'normal',
    size: null,
    auto_fit: true,
    color: [0, 0, 0, 255],
    stroke_color: [255, 255, 255, 255],
    stroke_width: 0,
    stroke_method: null,
    stroke_position: null,
    stroke_join: null,
    outer_stroke_color: null,
    outer_stroke_width: null,
    alignment: 'Center',
    writing_mode: null,
  },
  layout: 'paragraph',
  automatic_region: null,
}

const secondLayer: Layer = {
  ...textLayer,
  id: 'second',
  content: {
    ...textLayer.content,
    id: 'second-content',
    translation: { text: 'Second', language: null },
  },
}

const thirdLayer: Layer = {
  ...textLayer,
  id: 'third',
  content: {
    ...textLayer.content,
    id: 'third-content',
    translation: { text: 'Third', language: null },
  },
}

const artworkLayer: Layer = {
  type: 'artwork',
  id: 'artwork',
  parent: 'page',
  geometry: {
    points: [
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      { x: 10, y: 10 },
      { x: 0, y: 10 },
    ],
  },
  visibility: { visible: true, opacity: 1 },
  image: 'source',
}

const preferences: Preferences = {
  pipeline: {
    detect_text_color: true,
    default_text_color: null,
    detection: { model: 'koharu-layout-rfdetr-seg-2xl' },
    ocr: { model: 'paddleocr-vl-1.6' },
    translation: {
      model: {
        provider: 'local',
        model: 'gemma4-e2b-it',
        quantization: null,
        vision: true,
        reasoning: true,
      },
      generation: { vision: true, reasoning: false },
      target_language: 'en-US',
      instructions: null,
    },
    inpainting: { model: 'lama' },
    processor: {},
  },
  providers: {
    entries: [
      {
        name: 'Local',
        config: { provider: 'local', settings: {} },
        credential: null,
      },
      {
        name: 'OpenAI-compatible',
        config: {
          provider: 'openai-compatible',
          settings: { base_url: 'http://localhost:11434/v1' },
        },
        credential: emptyCredential(),
      },
      {
        name: 'LM Studio',
        config: { provider: 'lm-studio', settings: { base_url: 'http://localhost:1234' } },
        credential: emptyCredential(),
      },
      {
        name: 'DeepL',
        config: { provider: 'deepl', settings: { base_url: null } },
        credential: emptyCredential(),
      },
    ],
  },
  typesetting: {
    font_families: ['Noto Sans'],
  },
  languages: [
    { tag: 'en-US', name: 'English' },
    { tag: 'ja-JP', name: 'Japanese' },
  ],
}

function installProject() {
  const page = {
    id: 'page',
    label: 'Page 1',
    size: { width: 1000, height: 1500 },
    layers: [textLayer],
    regions: [],
  }
  queryClient.setQueryData(projectKey, {
    name: 'Book',
    revision: 1,
    active_page: 'page',
    can_undo: true,
    can_redo: false,
  })
  queryClient.setQueryData(pagesKey, [
    {
      id: 'page',
      label: 'Page 1',
      size: { width: 1000, height: 1500 },
      source_asset: 'source',
      layer_count: 1,
    },
  ])
  queryClient.setQueryData(pageKey, page)
  useKoharuStore.setState({
    preferences,
    translationModels: [
      {
        provider: 'local',
        model: 'gemma4-e2b-it',
        name: 'Gemma 4 E2B Instruct',
        quantizations: [],
        vision: true,
        reasoning: true,
      },
    ],
    selectedPages: ['page'],
    selectedLayers: ['element'],
    layerFrames: {
      element: { x: 10, y: 20, width: 100, height: 50, angle_degrees: 0 },
    },
  })
  vi.spyOn(commands, 'getPreferences').mockImplementation(
    async () => useKoharuStore.getState().preferences!,
  )
  vi.spyOn(commands, 'getTranslationModels').mockImplementation(async () => [
    ...useKoharuStore.getState().translationModels,
  ])
}

function render(ui: ReactNode) {
  return testingRender(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>)
}

function SettingsNavigationHarness() {
  const settingsOpen = useKoharuStore((state) => state.settingsOpen)
  return settingsOpen ? <SettingsPage /> : <CanvasCommandBar />
}

describe('greenfield editor', () => {
  it('registers one native resize listener in React strict mode', async () => {
    const unlisten = vi.fn()
    nativeWindow.onResized.mockClear()
    nativeWindow.onResized.mockResolvedValueOnce(unlisten)

    const view = render(
      <StrictMode>
        <WindowControls />
      </StrictMode>,
    )

    await waitFor(() => expect(nativeWindow.onResized).toHaveBeenCalledTimes(1))
    view.unmount()
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1))
  })

  it('starts native resize dragging from every frameless window edge', () => {
    nativeWindow.startResizeDragging.mockClear()
    const view = render(<WindowControls />)
    const directions = [
      'North',
      'South',
      'East',
      'West',
      'NorthEast',
      'NorthWest',
      'SouthEast',
      'SouthWest',
    ]

    for (const direction of directions) {
      fireEvent.pointerDown(
        view.container.querySelector(`[data-window-resize-handle="${direction}"]`)!,
        { button: 0 },
      )
    }

    expect(nativeWindow.startResizeDragging.mock.calls).toEqual(
      directions.map((direction) => [direction]),
    )
  })

  it('removes resize handles while the window is maximized', async () => {
    nativeWindow.isMaximized.mockResolvedValueOnce(true)
    const view = render(<WindowControls />)

    await waitFor(() =>
      expect(view.container.querySelector('[data-window-resize-handle]')).not.toBeInTheDocument(),
    )
  })

  it('shows import activity and prevents duplicate imports', async () => {
    const user = userEvent.setup()
    installProject()
    let finishImport: (() => void) | undefined
    const importPages = vi.spyOn(commands, 'import').mockImplementation(
      () =>
        new Promise<null>((resolve) => {
          finishImport = () => resolve(null)
        }),
    )
    render(
      <>
        <TitleBar />
        <PageRail />
        <ActivityCenter />
      </>,
    )

    expect(screen.getByText('/')).toHaveClass('mx-2')
    expect(screen.queryByRole('button', { name: 'Import pages' })).not.toBeInTheDocument()
    await user.click(screen.getByRole('menuitem', { name: 'File' }))
    await user.hover(await screen.findByRole('menuitem', { name: 'Import Pages' }))
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Files…' }))

    expect(await screen.findByRole('status')).toHaveTextContent('Importing pages…')
    expect(screen.getByRole('complementary', { name: 'Activity' })).toBeInTheDocument()
    expect(importPages).toHaveBeenCalledTimes(1)

    await user.click(screen.getByRole('menuitem', { name: 'File' }))
    expect(await screen.findByRole('menuitem', { name: 'Importing pages…' })).toHaveAttribute(
      'aria-disabled',
      'true',
    )

    finishImport?.()
    await waitFor(() => expect(screen.queryByRole('status')).not.toBeInTheDocument())
  })

  it.each(['png', 'psd', 'cbz'] as const)(
    'shows %s export activity and prevents duplicate exports',
    async (format) => {
      const user = userEvent.setup()
      installProject()
      let finishExport: (() => void) | undefined
      const exportProject = vi.spyOn(commands, 'export').mockImplementation(
        () =>
          new Promise<null>((resolve) => {
            finishExport = () => resolve(null)
          }),
      )
      render(
        <>
          <TitleBar />
          <ActivityCenter />
        </>,
      )

      expect(screen.queryByRole('complementary', { name: 'Activity' })).not.toBeInTheDocument()
      await user.click(screen.getByRole('menuitem', { name: 'File' }))
      await user.hover(await screen.findByRole('menuitem', { name: 'Export Project' }))
      fireEvent.click(await screen.findByRole('menuitem', { name: `${format.toUpperCase()}…` }))

      expect(await screen.findByRole('status')).toHaveTextContent('Export Project')
      expect(exportProject).toHaveBeenCalledExactlyOnceWith(format)
      await user.click(screen.getByRole('menuitem', { name: 'File' }))
      expect(await screen.findByRole('menuitem', { name: 'Export Project' })).toHaveAttribute(
        'aria-disabled',
        'true',
      )

      await act(async () => finishExport?.())
      await waitFor(() =>
        expect(screen.queryByRole('complementary', { name: 'Activity' })).not.toBeInTheDocument(),
      )
      expect(await screen.findByRole('menuitem', { name: 'Export Project' })).not.toHaveAttribute(
        'aria-disabled',
        'true',
      )
    },
  )

  it.each([
    { command: 'import', menu: 'Import Pages', choice: 'Files…', pending: 'Importing pages…' },
    { command: 'export', menu: 'Export Project', choice: 'CBZ…', pending: 'Export Project' },
  ] as const)(
    'clears $command activity after failure',
    async ({ command, menu, choice, pending }) => {
      const user = userEvent.setup()
      installProject()
      let fail: ((error: Error) => void) | undefined
      vi.spyOn(commands, command).mockImplementation(
        () =>
          new Promise<null>((_resolve, reject) => {
            fail = reject
          }),
      )
      render(
        <>
          <TitleBar />
          <ActivityCenter />
        </>,
      )

      await user.click(screen.getByRole('menuitem', { name: 'File' }))
      await user.hover(await screen.findByRole('menuitem', { name: menu }))
      fireEvent.click(await screen.findByRole('menuitem', { name: choice }))
      expect(await screen.findByRole('status')).toHaveTextContent(pending)

      await user.click(screen.getByRole('menuitem', { name: 'File' }))
      await act(async () => fail?.(new Error('File operation failed')))
      await waitFor(() =>
        expect(screen.queryByRole('complementary', { name: 'Activity' })).not.toBeInTheDocument(),
      )
      expect(await screen.findByRole('menuitem', { name: menu })).not.toHaveAttribute(
        'aria-disabled',
        'true',
      )
    },
  )

  it('opens community links through the Tauri opener plugin', async () => {
    nativeOpenUrl.mockClear()
    const user = userEvent.setup()
    render(<TitleBar />)

    await user.click(screen.getByRole('menuitem', { name: 'Help' }))
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Discord' }))
    expect(nativeOpenUrl).toHaveBeenLastCalledWith('https://discord.gg/mHvHkxGnUY')

    await user.click(screen.getByRole('menuitem', { name: 'Help' }))
    fireEvent.click(await screen.findByRole('menuitem', { name: 'GitHub' }))
    expect(nativeOpenUrl).toHaveBeenLastCalledWith('https://github.com/koharu-rs/koharu')
  })

  it('shows the current version and author in About', async () => {
    const user = userEvent.setup()
    render(<TitleBar />)

    await user.click(screen.getByRole('menuitem', { name: 'Help' }))
    await user.click(await screen.findByRole('menuitem', { name: 'About' }))

    expect(await screen.findByRole('heading', { name: 'Koharu' })).toBeInTheDocument()
    expect(await screen.findByText('0.62.0')).toBeInTheDocument()
    expect(screen.getByText('Mayo Takanashi')).toBeInTheDocument()
    expect(nativeGetVersion).toHaveBeenCalledTimes(1)
  })

  it('shows how many pages are selected above the filter', async () => {
    installProject()
    act(() => {
      useKoharuStore.setState({ selectedPages: ['page-1'] })
    })
    render(<PageRail />)

    // One page behaves like acting on the active page, so it is not worth
    // announcing.
    expect(screen.queryByRole('status')).not.toBeInTheDocument()

    act(() => {
      useKoharuStore.setState({ selectedPages: ['page-1', 'page-2', 'page-3'] })
    })

    // A status region, so the count reaches assistive technology when it
    // changes rather than only being visible.
    expect(await screen.findByRole('status')).toHaveTextContent('3 selected')
  })

  it('loads page thumbnails into the filmstrip', async () => {
    installProject()
    const thumbnail = vi.spyOn(commands, 'getThumbnail').mockResolvedValue([1])
    render(<PageRail />)
    await waitFor(() => expect(thumbnail).toHaveBeenCalledWith('page'))
    expect(await screen.findByRole('img', { name: 'Page 1' })).toHaveAttribute(
      'src',
      'blob:koharu-thumbnail',
    )
    expect(screen.queryByText('01')).not.toBeInTheDocument()
  })

  it('keeps rapid page switches on the latest native selection', async () => {
    installProject()
    const pages = [
      { id: 'page', label: 'Page 1', size: { width: 1000, height: 1500 }, layers: [], regions: [] },
      {
        id: 'page-2',
        label: 'Page 2',
        size: { width: 1000, height: 1500 },
        layers: [],
        regions: [],
      },
      {
        id: 'page-3',
        label: 'Page 3',
        size: { width: 1000, height: 1500 },
        layers: [],
        regions: [],
      },
    ]
    queryClient.setQueryData(
      pagesKey,
      pages.map((page) => ({
        id: page.id,
        label: page.label,
        size: page.size,
        source_asset: null,
        layer_count: 0,
      })),
    )
    vi.spyOn(canvasRuntime, 'showCanvasPage').mockReturnValue(false)
    const pending = new Map<
      string,
      (selection: Awaited<ReturnType<typeof commands.selectPage>>) => void
    >()
    const selectPage = vi.spyOn(commands, 'selectPage').mockImplementation(
      (page) =>
        new Promise((resolve) => {
          pending.set(page, resolve)
        }),
    )
    render(<PageRail />)

    fireEvent.click(screen.getByText('Page 2').closest('article')!)
    expect(selectPage).toHaveBeenLastCalledWith('page-2')

    fireEvent.click(screen.getByText('Page 3').closest('article')!)
    expect(selectPage).toHaveBeenLastCalledWith('page-3')

    await act(async () => {
      pending.get('page-3')!({
        project: {
          name: 'Book',
          revision: 1,
          active_page: 'page-3',
          can_undo: true,
          can_redo: false,
        },
        page: pages[2]!,
      })
      await Promise.resolve()
    })
    await act(async () => {
      pending.get('page-2')!({
        project: {
          name: 'Book',
          revision: 1,
          active_page: 'page-2',
          can_undo: true,
          can_redo: false,
        },
        page: pages[1]!,
      })
      await Promise.resolve()
    })

    expect(queryClient.getQueryData<{ active_page: string }>(projectKey)?.active_page).toBe(
      'page-3',
    )
    expect(queryClient.getQueryData<{ id: string }>(pageKey)?.id).toBe('page-3')
  })

  it('lets a cached canvas frame paint before synchronizing native page state', async () => {
    installProject()
    queryClient.setQueryData(pagesKey, [
      ...(queryClient.getQueryData<PageSummary[]>(pagesKey) ?? []),
      {
        id: 'page-2',
        label: 'Page 2',
        size: { width: 1000, height: 1500 },
        source_asset: null,
        layer_count: 0,
      },
    ])
    vi.spyOn(canvasRuntime, 'showCanvasPage').mockReturnValue(true)
    const selectPage = vi.spyOn(commands, 'selectPage')
    let paint: FrameRequestCallback | undefined
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      paint = callback
      return 1
    })
    render(<PageRail />)

    fireEvent.click(screen.getByText('Page 2').closest('article')!)

    expect(selectPage).not.toHaveBeenCalled()
    await act(async () => {
      paint?.(performance.now())
      await new Promise((resolve) => setTimeout(resolve, 0))
    })
    expect(selectPage).toHaveBeenCalledWith('page-2')
  })

  it('prefetches an inactive page on pointer intent and caches its preparation', async () => {
    installProject()
    const page = {
      id: 'page-2',
      label: 'Page 2',
      size: { width: 1000, height: 1500 },
      layers: [],
      regions: [],
    }
    queryClient.setQueryData(pagesKey, [
      ...(queryClient.getQueryData<PageSummary[]>(pagesKey) ?? []),
      {
        id: page.id,
        label: page.label,
        size: page.size,
        source_asset: null,
        layer_count: 0,
      },
    ])
    const prepared = { revision: 1, page }
    const prefetch = vi.spyOn(canvasRuntime, 'prefetchCanvasPages').mockResolvedValue([prepared])
    const selectPage = vi.spyOn(commands, 'selectPage')
    render(<PageRail />)

    fireEvent.pointerEnter(screen.getByText('Page 2').closest('article')!)

    await waitFor(() => expect(prefetch).toHaveBeenCalledWith(['page-2']))
    await waitFor(() =>
      expect(queryClient.getQueryData(preparedPageKey('page-2'))).toEqual(prepared),
    )
    expect(selectPage).not.toHaveBeenCalled()
  })

  it('deduplicates page intent within a project revision and retries on a newer revision', async () => {
    installProject()
    const page = {
      id: 'page-2',
      label: 'Page 2',
      size: { width: 1000, height: 1500 },
      layers: [],
      regions: [],
    }
    queryClient.setQueryData(pagesKey, [
      ...(queryClient.getQueryData<PageSummary[]>(pagesKey) ?? []),
      {
        id: page.id,
        label: page.label,
        size: page.size,
        source_asset: null,
        layer_count: 0,
      },
    ])
    const prefetch = vi
      .spyOn(canvasRuntime, 'prefetchCanvasPages')
      .mockResolvedValueOnce([{ revision: 1, page }])
      .mockResolvedValueOnce([{ revision: 2, page }])
    render(<PageRail />)
    const item = screen.getByText('Page 2').closest('article')!

    fireEvent.pointerEnter(item)
    fireEvent.focus(screen.getByRole('button', { name: 'Actions for Page 2' }))
    await waitFor(() => expect(prefetch).toHaveBeenCalledTimes(1))
    fireEvent.pointerEnter(item)
    expect(prefetch).toHaveBeenCalledTimes(1)

    queryClient.setQueryData(projectKey, {
      ...queryClient.getQueryData<ProjectInfo>(projectKey)!,
      revision: 2,
    })
    fireEvent.focus(item)

    await waitFor(() => expect(prefetch).toHaveBeenCalledTimes(2))
    expect(prefetch).toHaveBeenLastCalledWith(['page-2'])
  })

  it('switches tools and applies typography from the contextual inspector', async () => {
    const user = userEvent.setup()
    installProject()
    const setTypography = vi.spyOn(commands, 'setTypography').mockResolvedValue(null)
    render(
      <TooltipProvider>
        <ToolBar />
        <Inspector />
      </TooltipProvider>,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Brush' }))
    expect(useKoharuStore.getState().tool).toBe('draw')
    fireEvent.click(screen.getByRole('button', { name: 'Text' }))
    expect(screen.getByTestId('type-inspector')).toBeInTheDocument()
    expect(screen.getByTestId('type-font-picker')).toHaveTextContent('Noto Sans')
    expect(screen.getByTestId('type-size')).toHaveValue('')
    expect(screen.getByTestId('type-size')).toHaveAttribute('placeholder', 'Auto')
    expect(screen.getByRole('combobox', { name: 'Text direction' })).toHaveTextContent('Auto')
    await user.clear(screen.getByTestId('type-size'))
    await user.type(screen.getByTestId('type-size'), '18')
    await user.tab()
    await waitFor(() =>
      expect(setTypography).toHaveBeenCalledWith(
        expect.arrayContaining([
          expect.objectContaining({
            layer: 'element',
            typography: expect.objectContaining({ size: 18, writing_mode: null }),
          }),
        ]),
      ),
    )
  })

  it('defaults vertical text alignment to top and maps end to bottom', async () => {
    const user = userEvent.setup()
    installProject()
    queryClient.setQueryData(pageKey, (page: { layers: Layer[] }) => ({
      ...page,
      layers: page.layers.map((layer) =>
        layer.type === 'text'
          ? {
              ...layer,
              typography: { ...layer.typography, alignment: null, writing_mode: 'Vertical' },
            }
          : layer,
      ),
    }))
    const setTypography = vi.spyOn(commands, 'setTypography').mockResolvedValue(null)
    render(<Inspector />)

    expect(screen.getByRole('button', { name: 'Align top' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    await user.click(screen.getByRole('button', { name: 'Align bottom' }))
    await waitFor(() =>
      expect(setTypography).toHaveBeenCalledWith([
        expect.objectContaining({
          layer: 'element',
          typography: expect.objectContaining({ alignment: 'End', writing_mode: 'Vertical' }),
        }),
      ]),
    )
  })

  it('adjusts brush size from the toolbar popover', async () => {
    const user = userEvent.setup()
    installProject()
    useKoharuStore.setState({ tool: 'draw' })
    render(
      <TooltipProvider>
        <ToolBar />
      </TooltipProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Brush size: 48 pixels' }))
    expect(screen.getByRole('textbox', { name: 'Brush size' })).toHaveValue('48')

    await user.click(screen.getByRole('button', { name: 'Increase brush size' }))
    expect(useKoharuStore.getState().brush.diameter).toBe(49)
  })

  it('uses the border color well to enable and disable the border', async () => {
    const user = userEvent.setup()
    installProject()
    const setTypography = vi.spyOn(commands, 'setTypography').mockResolvedValue(null)
    render(<Inspector />)

    expect(screen.queryByRole('button', { name: 'Enable text border' })).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Border color' }))
    await user.click(screen.getByRole('button', { name: 'Transparent' }))
    await waitFor(() =>
      expect(setTypography).toHaveBeenCalledWith([
        expect.objectContaining({
          layer: 'element',
          typography: expect.objectContaining({
            stroke_color: [255, 255, 255, 0],
            stroke_width: 1.5,
          }),
        }),
      ]),
    )

    setTypography.mockClear()
    fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
      target: { value: '#FF0000' },
    })
    await waitFor(() =>
      expect(setTypography).toHaveBeenCalledWith([
        expect.objectContaining({
          layer: 'element',
          typography: expect.objectContaining({
            stroke_color: [255, 0, 0, 255],
            stroke_width: 1.5,
          }),
        }),
      ]),
    )
  })

  it('gates the second outline on the first and can enable it by width', async () => {
    const user = userEvent.setup()
    installProject()
    const setTypography = vi.spyOn(commands, 'setTypography').mockResolvedValue(null)
    render(<Inspector />)

    expect(screen.getByRole('button', { name: 'Second border color' })).toBeDisabled()
    expect(screen.getByRole('textbox', { name: 'Second border width' })).toBeDisabled()

    act(() => {
      queryClient.setQueryData(pageKey, {
        id: 'page',
        label: 'Page 1',
        size: { width: 1000, height: 1500 },
        layers: [{ ...textLayer, typography: { ...textLayer.typography!, stroke_width: 4 } }],
        regions: [],
      })
    })
    await waitFor(() =>
      expect(screen.getByRole('textbox', { name: 'Second border width' })).toBeEnabled(),
    )
    await user.click(screen.getByRole('button', { name: 'Increase second border width' }))
    await waitFor(() => expect(setTypography).toHaveBeenCalledWith([
      expect.objectContaining({
        layer: 'element',
        typography: expect.objectContaining({ stroke_width: 4, outer_stroke_width: 3.5 }),
      }),
    ]))
  })

  it('enables and clears the second outline without changing the first', async () => {
    const user = userEvent.setup()
    installProject()
    const layer = {
      ...textLayer,
      typography: { ...textLayer.typography!, stroke_width: 4 },
    }
    queryClient.setQueryData(pageKey, {
      id: 'page', label: 'Page 1', size: { width: 1000, height: 1500 },
      layers: [layer], regions: [],
    })
    const setTypography = vi.spyOn(commands, 'setTypography').mockResolvedValue(null)
    render(<Inspector />)

    await user.click(screen.getByRole('button', { name: 'Second border color' }))
    fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
      target: { value: '#FF0000' },
    })
    await waitFor(() => expect(setTypography).toHaveBeenCalledWith([
      expect.objectContaining({
        layer: 'element',
        typography: expect.objectContaining({
          stroke_color: [255, 255, 255, 255], stroke_width: 4,
          outer_stroke_color: [255, 0, 0, 255], outer_stroke_width: 3,
        }),
      }),
    ]))

    setTypography.mockClear()
    await user.click(screen.getByRole('button', { name: 'Transparent' }))
    await waitFor(() => expect(setTypography).toHaveBeenCalledWith([
      expect.objectContaining({
        layer: 'element',
        typography: expect.objectContaining({ stroke_width: 4, outer_stroke_width: 0 }),
      }),
    ]))
  })

  it('only offers styles and weights available for the selected font family', async () => {
    const user = userEvent.setup()
    installProject()
    const setTypography = vi.spyOn(commands, 'setTypography').mockResolvedValue(null)
    queryClient.setQueryData(fontsKey, [
      {
        name: 'Noto Sans',
        metadata: {
          primary_script: 'latn',
          scripts: ['latn'],
          languages: ['en'],
          category: 'SANS_SERIF',
          classifications: ['sans-serif'],
          use_cases: ['body-text'],
        },
        sources: ['system'],
        faces: [
          {
            postscript_name: 'NotoSans-Regular',
            weight: 400,
            weight_range: null,
            style: 'normal',
          },
          {
            postscript_name: 'NotoSans-Bold',
            weight: 700,
            weight_range: null,
            style: 'normal',
          },
          {
            postscript_name: 'NotoSans-Italic',
            weight: 400,
            weight_range: null,
            style: 'italic',
          },
        ],
      },
    ])
    const { unmount } = render(<Inspector />)

    fireEvent.click(screen.getByRole('combobox', { name: 'Font weight' }))

    expect(screen.getByRole('option', { name: '400' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: '700' })).toBeInTheDocument()
    expect(screen.queryByRole('option', { name: '100' })).not.toBeInTheDocument()

    unmount()
    render(<Inspector />)
    fireEvent.click(screen.getByRole('combobox', { name: 'Font style' }))
    expect(screen.getByRole('option', { name: 'Regular' })).toBeInTheDocument()
    await user.click(screen.getByRole('option', { name: 'Italic' }))
    await waitFor(() =>
      expect(setTypography).toHaveBeenCalledWith([
        expect.objectContaining({
          layer: 'element',
          typography: expect.objectContaining({ font_style: 'italic', font_weight: 400 }),
        }),
      ]),
    )
  })

  it('debounces layer text editing and flushes it when focus leaves the field', async () => {
    installProject()
    const save = vi.spyOn(commands, 'setSourceText').mockResolvedValue(null)
    render(<Inspector />)
    const layer = screen.getByRole('button', { name: 'Edit Hello' })
    expect(screen.getByTestId('edit-source-element')).toBeInTheDocument()
    fireEvent.click(layer)
    expect(screen.queryByTestId('edit-source-element')).not.toBeInTheDocument()
    fireEvent.click(layer)
    const source = screen.getByTestId('edit-source-element')
    fireEvent.change(source, { target: { value: 'corrected OCR' } })
    fireEvent.blur(source)
    await waitFor(() => expect(save).toHaveBeenCalledWith('element', 'corrected OCR'))
  })

  it('shows actual layers with only the useful text-role distinction', () => {
    installProject()
    queryClient.setQueryData(pageKey, (page: { layers: Layer[] }) => ({
      ...page,
      layers: [
        ...page.layers.map((layer) =>
          layer.type === 'text'
            ? { ...layer, content: { ...layer.content, role: 'dev.koharu.text.onomatopoeia' } }
            : layer,
        ),
        {
          ...textLayer,
          id: 'dialogue',
          content: {
            ...textLayer.content,
            id: 'dialogue-content',
            translation: { text: 'Dialogue line', language: null },
            role: 'dev.koharu.text.dialogue',
          },
        },
        {
          ...textLayer,
          id: 'free-text',
          content: {
            ...textLayer.content,
            id: 'free-text-content',
            translation: { text: 'Caption', language: null },
            role: 'dev.koharu.text.free-text',
          },
        },
      ],
    }))
    render(<Inspector />)

    expect(screen.queryByRole('button', { name: /Filter layers by type/ })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Edit Hello' })).toHaveTextContent('Text')
    expect(screen.getByRole('button', { name: 'Edit Dialogue line' })).toHaveTextContent('Dialogue')
    expect(screen.getByRole('button', { name: 'Edit Caption' })).toHaveTextContent('Free text')
    expect(screen.queryByText('Onomatopoeia')).not.toBeInTheDocument()
  })

  function installLayerSelectionFixture() {
    installProject()
    queryClient.setQueryData(pageKey, (page: { layers: Layer[] }) => ({
      ...page,
      layers: [...page.layers, secondLayer, artworkLayer, thirdLayer],
    }))
  }

  it('toggles layers in the selection with ctrl-click', () => {
    installLayerSelectionFixture()
    render(<Inspector />)

    expect(useKoharuStore.getState().selectedLayers).toEqual(['element'])
    fireEvent.click(screen.getByRole('button', { name: 'Edit Second' }), { ctrlKey: true })
    expect(useKoharuStore.getState().selectedLayers).toEqual(['element', 'second'])
    fireEvent.click(screen.getByRole('button', { name: 'Edit Hello' }), { ctrlKey: true })
    expect(useKoharuStore.getState().selectedLayers).toEqual(['second'])
    fireEvent.click(screen.getByRole('button', { name: 'Edit Second' }), { ctrlKey: true })
    expect(useKoharuStore.getState().selectedLayers).toEqual([])
  })

  it('toggles layers in the selection with meta-click', () => {
    installLayerSelectionFixture()
    render(<Inspector />)

    fireEvent.click(screen.getByRole('button', { name: 'Edit Second' }), { metaKey: true })
    expect(useKoharuStore.getState().selectedLayers).toEqual(['element', 'second'])
    fireEvent.click(screen.getByRole('button', { name: 'Edit Second' }), { metaKey: true })
    expect(useKoharuStore.getState().selectedLayers).toEqual(['element'])
  })

  it('selects a display range with shift-click, skipping locked layers', () => {
    installLayerSelectionFixture()
    render(<Inspector />)

    fireEvent.click(screen.getByRole('button', { name: 'Edit Third' }))
    expect(useKoharuStore.getState().selectedLayers).toEqual(['third'])
    fireEvent.click(screen.getByRole('button', { name: 'Edit Hello' }), { shiftKey: true })
    expect(useKoharuStore.getState().selectedLayers).toEqual(['third', 'second', 'element'])
  })

  it('unions a shift range with the existing selection when ctrl is held', () => {
    installLayerSelectionFixture()
    render(<Inspector />)

    fireEvent.click(screen.getByRole('button', { name: 'Edit Second' }))
    fireEvent.click(screen.getByRole('button', { name: 'Edit Third' }), {
      ctrlKey: true,
      shiftKey: true,
    })
    expect(useKoharuStore.getState().selectedLayers).toEqual(['second', 'third'])
  })

  it('falls back to single selection when shift-clicking without an anchor', () => {
    installLayerSelectionFixture()
    render(<Inspector />)

    fireEvent.click(screen.getByRole('button', { name: 'Edit Third' }), { shiftKey: true })
    expect(useKoharuStore.getState().selectedLayers).toEqual(['third'])
  })

  it('collapses a multi-selection back to a single layer on plain click', () => {
    installLayerSelectionFixture()
    render(<Inspector />)

    fireEvent.click(screen.getByRole('button', { name: 'Edit Second' }), { ctrlKey: true })
    expect(useKoharuStore.getState().selectedLayers).toEqual(['element', 'second'])
    fireEvent.click(screen.getByRole('button', { name: 'Edit Hello' }))
    expect(useKoharuStore.getState().selectedLayers).toEqual(['element'])
  })

  it('fixes only rendered auto-fit siblings in the same bubble with one canvas commit', async () => {
    installProject()
    const auto = { ...textLayer, geometry: null, parent: 'text-group', automatic_region: 'bubble' }
    const layers: Layer[] = [
      { type: 'group', id: 'text-group', parent: 'page', name: 'Text', role: 'text',
        visibility: { visible: true, opacity: 1 } },
      auto,
      { ...auto, id: 'rotated' },
      { ...auto, id: 'no-frame' },
      { ...auto, id: 'fixed', geometry: textLayer.geometry },
      { ...auto, id: 'different-bubble', automatic_region: 'other-bubble' },
    ]
    queryClient.setQueryData(pageKey, (page: { layers: Layer[] }) => ({ ...page, layers }))
    const frames = {
      element: { x: 30, y: 40, width: 50, height: 20, angle_degrees: 0 },
      rotated: { x: 90, y: 40, width: 40, height: 20, angle_degrees: 27 },
      fixed: { x: 10, y: 10, width: 20, height: 20, angle_degrees: 0 },
      'different-bubble': { x: 10, y: 80, width: 20, height: 20, angle_degrees: 0 },
    }
    useKoharuStore.setState({ canvasPage: 'page', canvasRevision: 1, layerFrames: frames })
    const commit = vi.spyOn(commands, 'commitTransform').mockResolvedValue(2)
    render(<Inspector />)
    fireEvent.click(screen.getByRole('button', { name: 'Fix all in bubble' }))
    await waitFor(() => expect(commit).toHaveBeenCalledExactlyOnceWith(1, [
      { element: 'element', frame: frames.element },
      { element: 'rotated', frame: frames.rotated },
    ], true))
    act(() => queryClient.setQueryData(pageKey, (page: { layers: Layer[] }) => ({
      ...page, layers: layers.map((layer) => layer.type === 'text'
        ? { ...layer, geometry: textLayer.geometry } : layer),
    })))
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Fix all in bubble' })).not.toBeInTheDocument())
  })

  it('resets a custom text frame to its automatic region', async () => {
    installProject()
    queryClient.setQueryData(pageKey, (page: { layers: Layer[] }) => ({
      ...page,
      layers: page.layers.map((layer) =>
        layer.type === 'text' ? { ...layer, automatic_region: 'bubble' } : layer,
      ),
    }))
    const reset = vi.spyOn(commands, 'setGeometry').mockResolvedValue(null)
    render(<Inspector />)

    expect(screen.getByText('Custom frame')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Reset to auto fit' }))

    await waitFor(() => expect(reset).toHaveBeenCalledWith([{ layer: 'element', points: null }]))
  })

  it('shows zoom before page size without a fit control', () => {
    installProject()
    useKoharuStore.setState({ camera: { zoom: 1.25, translation: [0, 0], fitted: false } })
    render(<StatusBar onZoomChange={vi.fn()} />)

    const zoom = screen.getByText('125%')
    const size = screen.getByText('1000 × 1500 px')
    expect(zoom.compareDocumentPosition(size) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0)
    expect(screen.queryByRole('button', { name: 'Fit window' })).not.toBeInTheDocument()
  })

  it('changes the pipeline scope and selected stages from the runtime selector', async () => {
    installProject()
    const run = vi.spyOn(commands, 'process').mockResolvedValue('job')
    render(<CanvasCommandBar />)

    fireEvent.click(screen.getByRole('button', { name: 'Processing settings' }))
    fireEvent.click(screen.getByRole('button', { name: /Scope Selection/ }))
    fireEvent.click(screen.getByRole('button', { name: /Entire project/ }))
    fireEvent.click(screen.getByRole('button', { name: /Stages 4 stages/ }))
    fireEvent.click(screen.getByRole('button', { name: /Translation/ }))
    fireEvent.click(screen.getByRole('button', { name: /Inpainting/ }))
    fireEvent.click(screen.getByRole('button', { name: 'Run processing' }))
    await waitFor(() =>
      expect(run).toHaveBeenLastCalledWith(
        { scope: 'project' },
        { operation: 'stages', stages: ['detection', 'ocr'] },
      ),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Processing settings' }))
    fireEvent.click(screen.getByRole('button', { name: /Scope Project/ }))
    fireEvent.click(screen.getByRole('button', { name: /Selected pages/ }))
    fireEvent.click(screen.getByRole('button', { name: /Stages 2 stages/ }))
    fireEvent.click(screen.getByRole('button', { name: /Translation/ }))
    fireEvent.click(screen.getByRole('button', { name: /Inpainting/ }))
    fireEvent.click(screen.getByRole('button', { name: 'Run processing' }))
    await waitFor(() =>
      expect(run).toHaveBeenLastCalledWith(
        { scope: 'pages', value: ['page'] },
        { operation: 'full' },
      ),
    )
  })

  it('runs the selected pages by default and exposes the runtime shortcuts', async () => {
    installProject()
    const run = vi.spyOn(commands, 'process').mockResolvedValue('job')
    render(<CanvasCommandBar />)

    fireEvent.click(screen.getByRole('button', { name: 'Run processing' }))
    await waitFor(() =>
      expect(run).toHaveBeenLastCalledWith(
        { scope: 'pages', value: ['page'] },
        { operation: 'full' },
      ),
    )

    const selector = screen.getByRole('button', { name: 'Processing settings' })
    const runButton = screen.getByRole('button', { name: 'Run processing' })
    expect(selector).toHaveClass('h-7')
    expect(runButton).toHaveClass('h-7', 'bg-primary/80', 'hover:bg-primary/90')
    fireEvent.click(selector)
    await waitFor(() => expect(commands.getTranslationModels).toHaveBeenCalled())
    expect(screen.getByRole('button', { name: /Model Gemma 4 E2B Instruct/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Scope Selection/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Stages 4 stages/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Output English/ })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    expect(useKoharuStore.getState().settingsOpen).toBe(true)
  })

  it('configures translation output from the runtime selector', async () => {
    installProject()
    const user = userEvent.setup()
    const nextPreferences: Preferences = {
      ...preferences,
      pipeline: {
        ...preferences.pipeline,
        translation: {
          ...preferences.pipeline.translation,
          target_language: 'ja-JP',
          instructions: 'Keep character names unchanged.',
        },
      },
    }
    let finishSave!: (saved: Preferences) => void
    const pendingSave = new Promise<Preferences>((resolve) => {
      finishSave = resolve
    })
    const save = vi.spyOn(commands, 'savePreferences').mockImplementation(() => pendingSave)
    render(<CanvasCommandBar />)

    await user.click(screen.getByRole('button', { name: 'Processing settings' }))
    await waitFor(() => expect(commands.getTranslationModels).toHaveBeenCalled())
    await user.click(screen.getByRole('button', { name: /Output English/ }))
    const language = screen.getByRole('combobox', { name: 'Target language' })
    expect(language).toHaveTextContent('English')
    expect(language).not.toHaveTextContent('en-US')
    await user.click(language)
    await user.click(await screen.findByRole('option', { name: 'Japanese' }))
    const instructions = screen.getByRole('textbox', { name: 'Translation instructions' })
    expect(instructions).toHaveClass('max-h-20', 'overflow-y-auto')
    await user.type(instructions, 'Keep character names unchanged.')
    expect(screen.queryByRole('button', { name: 'Apply output' })).not.toBeInTheDocument()

    await waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        nextPreferences.pipeline,
        preferences.providers,
        preferences.typesetting,
      ),
    )
    expect(
      screen.queryByLabelText(/Saving output settings|outputPicker\.saving/),
    ).not.toBeInTheDocument()
    expect(language).not.toBeDisabled()
    expect(instructions).not.toBeDisabled()
    expect(instructions).toHaveFocus()
    await act(async () => finishSave(nextPreferences))
    await waitFor(() =>
      expect(useKoharuStore.getState().preferences?.pipeline.translation).toEqual(
        nextPreferences.pipeline.translation,
      ),
    )
    expect(instructions).toBeInTheDocument()
    expect(instructions).toHaveFocus()
  })

  it('preserves the runtime shortcuts while visiting settings', async () => {
    installProject()
    const user = userEvent.setup()
    const save = vi
      .spyOn(commands, 'savePreferences')
      .mockImplementation(async (pipeline, providers, typesetting) => ({
        ...preferences,
        pipeline,
        providers,
        typesetting,
      }))
    render(
      <ThemeProvider attribute='class'>
        <SettingsNavigationHarness />
      </ThemeProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Processing settings' }))
    await user.click(screen.getByRole('button', { name: /Scope Selection/ }))
    await user.click(screen.getByRole('button', { name: /Entire project/ }))
    await user.click(screen.getByRole('button', { name: /Stages 4 stages/ }))
    await user.click(screen.getByRole('button', { name: /Translation/ }))
    await user.click(screen.getByRole('button', { name: /Inpainting/ }))
    await user.click(screen.getByRole('button', { name: 'Back' }))
    await user.click(screen.getByRole('button', { name: /Output English/ }))
    await user.click(screen.getByRole('combobox', { name: 'Target language' }))
    await user.click(await screen.findByRole('option', { name: 'Japanese' }))

    act(() => useKoharuStore.getState().setSettingsOpen(true))
    await waitFor(() => expect(save).toHaveBeenCalled())
    await user.click(screen.getByRole('button', { name: 'Back to editor' }))
    await user.click(screen.getByRole('button', { name: 'Processing settings' }))

    expect(screen.getByRole('button', { name: /Scope Project/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Stages 2 stages/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Output Japanese/ })).toBeInTheDocument()
  })

  it('changes the translation model without re-enabling vision or reasoning', async () => {
    installProject()
    const user = userEvent.setup()
    const currentPreferences: Preferences = {
      ...preferences,
      pipeline: {
        ...preferences.pipeline,
        translation: {
          ...preferences.pipeline.translation,
          generation: {
            ...preferences.pipeline.translation.generation,
            vision: false,
            reasoning: false,
          },
        },
      },
    }
    useKoharuStore.setState({ preferences: currentPreferences })
    const nextPreferences: Preferences = {
      ...currentPreferences,
      pipeline: {
        ...currentPreferences.pipeline,
        translation: {
          ...currentPreferences.pipeline.translation,
          model: {
            provider: 'local',
            model: 'gemma4-12b-it',
            quantization: null,
            vision: true,
            reasoning: true,
          },
        },
      },
    }
    const save = vi.spyOn(commands, 'savePreferences').mockResolvedValue(nextPreferences)
    useKoharuStore.setState({
      translationModels: [
        ...useKoharuStore.getState().translationModels,
        {
          provider: 'local',
          model: 'gemma4-12b-it',
          name: 'Gemma 4 12B',
          quantizations: [],
          vision: true,
          reasoning: true,
        },
      ],
    })
    render(<CanvasCommandBar />)

    fireEvent.click(screen.getByRole('button', { name: 'Processing settings' }))
    fireEvent.click(screen.getByRole('button', { name: /Model Gemma 4 E2B Instruct/ }))
    const search = screen.getByRole('textbox', { name: 'Search models' })
    expect(search).toHaveFocus()
    await user.type(search, '12b')
    expect(
      screen.queryByRole('button', { name: 'Use Gemma 4 E2B Instruct from Local' }),
    ).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Use Gemma 4 12B from Local' }))

    await waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        nextPreferences.pipeline,
        currentPreferences.providers,
        currentPreferences.typesetting,
      ),
    )
    expect(useKoharuStore.getState().preferences?.pipeline.translation).toEqual(
      nextPreferences.pipeline.translation,
    )
  })

  it('preserves vision when the runtime selector chooses a text-only model', async () => {
    installProject()
    const nextPreferences: Preferences = {
      ...preferences,
      pipeline: {
        ...preferences.pipeline,
        translation: {
          ...preferences.pipeline.translation,
          model: {
            provider: 'deepseek',
            model: 'deepseek-chat',
            quantization: null,
            vision: false,
            reasoning: true,
          },
          generation: {
            ...preferences.pipeline.translation.generation,
            vision: true,
            reasoning: false,
          },
        },
      },
    }
    const save = vi.spyOn(commands, 'savePreferences').mockResolvedValue(nextPreferences)
    useKoharuStore.setState({
      translationModels: [
        ...useKoharuStore.getState().translationModels,
        {
          provider: 'deepseek',
          model: 'deepseek-chat',
          name: 'DeepSeek Chat',
          quantizations: [],
          vision: false,
          reasoning: true,
        },
      ],
    })
    render(<CanvasCommandBar />)

    fireEvent.click(screen.getByRole('button', { name: 'Processing settings' }))
    fireEvent.click(screen.getByRole('button', { name: /Model Gemma 4 E2B Instruct/ }))
    fireEvent.click(screen.getByRole('button', { name: 'Use DeepSeek Chat from deepseek' }))

    await waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        nextPreferences.pipeline,
        preferences.providers,
        preferences.typesetting,
      ),
    )
    expect(useKoharuStore.getState().preferences?.pipeline.translation).toEqual(
      nextPreferences.pipeline.translation,
    )
  })

  it('constrains long model names inside the runtime selector', async () => {
    installProject()
    const longName = 'Llama 3.2 8x3b Moe Dark Champion Instruct Uncensored Abliterated 18.4b'
    useKoharuStore.setState({
      translationModels: [
        ...useKoharuStore.getState().translationModels,
        {
          provider: 'local',
          model: 'long-model',
          name: longName,
          quantizations: [],
          vision: true,
          reasoning: false,
        },
      ],
    })
    render(<CanvasCommandBar />)

    fireEvent.click(screen.getByRole('button', { name: 'Processing settings' }))
    fireEvent.click(screen.getByRole('button', { name: /Model Gemma 4 E2B Instruct/ }))

    const label = await screen.findByText(longName)
    await waitFor(() => expect(commands.getTranslationModels).toHaveBeenCalled())
    expect(label).toHaveClass('truncate')
    expect(label.parentElement).toHaveClass('overflow-hidden')
    expect(label.closest('button')).toHaveClass('max-w-full', 'min-w-0', 'overflow-hidden')
    const content = label.closest('[data-slot="scroll-area-content"]')
    expect(content).toHaveStyle({ width: '100%' })
    expect(content?.closest('[data-slot="scroll-area-viewport"]')).toHaveClass('h-auto', 'max-h-64')
  })

  it('edits and persists pipeline and translation preferences from the settings page', async () => {
    installProject()
    const user = userEvent.setup()
    useKoharuStore.setState({ settingsOpen: true })
    const save = vi.spyOn(commands, 'savePreferences').mockResolvedValue(preferences)
    render(
      <ThemeProvider attribute='class'>
        <SettingsPage />
      </ThemeProvider>,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Pipeline' }))
    expect(screen.getByRole('heading', { level: 2, name: 'Pipeline' })).toBeInTheDocument()
    expect(screen.getAllByRole('combobox')).toHaveLength(3)
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Text threshold' }), {
      target: { value: '0.42' },
    })
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        expect.objectContaining({
          processor: expect.objectContaining({
            'koharu-layout-rfdetr-seg-2xl': expect.objectContaining({ text_threshold: 0.42 }),
          }),
        }),
        preferences.providers,
        preferences.typesetting,
      ),
    )
    fireEvent.click(screen.getByRole('button', { name: 'Providers' }))
    expect(screen.getByRole('heading', { level: 2, name: 'Providers' })).toBeInTheDocument()
    expect(screen.getByLabelText('DeepL credential')).toBeInTheDocument()
    expect(screen.getAllByLabelText('Base URL')).toHaveLength(3)
    expect(screen.queryByRole('switch', { name: 'Vision' })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Translation' }))
    expect(screen.getByRole('heading', { level: 2, name: 'Translation' })).toBeInTheDocument()
    expect(screen.getByText('Choose the model used to translate text.')).toBeInTheDocument()
    expect(
      screen.getByText('Control how the model selects and varies generated text.'),
    ).toBeInTheDocument()
    expect(screen.getByText('Use model reasoning during translation.')).toBeInTheDocument()
    const reasoning = screen.getByRole('switch', { name: 'Enable reasoning' })
    expect(reasoning).not.toHaveAttribute('aria-disabled', 'true')
    expect(screen.queryByText('Enable reasoning')).not.toBeInTheDocument()
    save.mockClear()
    await user.click(reasoning)
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        expect.objectContaining({
          translation: expect.objectContaining({
            generation: expect.objectContaining({ reasoning: true }),
          }),
        }),
        preferences.providers,
        preferences.typesetting,
      ),
    )
    const vision = screen.getByRole('switch', { name: 'Vision' })
    expect(screen.getByText('Feed page images to the LLM during translation.')).toBeInTheDocument()
    expect(vision).toBeChecked()
    save.mockClear()
    await user.click(vision)
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        expect.objectContaining({
          translation: expect.objectContaining({
            generation: expect.objectContaining({ vision: false }),
          }),
        }),
        preferences.providers,
        preferences.typesetting,
      ),
    )
    expect(screen.getByLabelText('Translation model')).toHaveTextContent('Gemma 4 E2B Instruct')
    await user.click(screen.getByLabelText('Translation model'))
    const modelSearch = screen.getByRole('textbox', { name: 'Search models' })
    expect(modelSearch).toHaveFocus()
    await user.type(modelSearch, 'missing model')
    expect(screen.getByText('No models match this search.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Clear search' }))
    expect(
      screen.getByRole('button', { name: 'Use Gemma 4 E2B Instruct from Local' }),
    ).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Back' }))
    expect(screen.getByLabelText('Target language')).toHaveTextContent('English')
    expect(screen.getByText('Choose the language to translate text into.')).toBeInTheDocument()
    expect(screen.getByLabelText('Translation instructions')).toHaveClass(
      'field-sizing-fixed',
      'overflow-y-auto',
    )
  })

  it('keeps generation controls independent from model capabilities', async () => {
    installProject()
    const user = userEvent.setup()
    const configured: Preferences = {
      ...preferences,
      pipeline: {
        ...preferences.pipeline,
        translation: {
          ...preferences.pipeline.translation,
          model: {
            provider: 'deepseek',
            model: 'deepseek-chat',
            quantization: null,
            vision: false,
            reasoning: true,
          },
          generation: {
            ...preferences.pipeline.translation.generation,
            reasoning: false,
          },
        },
      },
    }
    useKoharuStore.setState({
      settingsOpen: true,
      preferences: configured,
      translationModels: [
        {
          provider: 'deepseek',
          model: 'deepseek-chat',
          name: 'DeepSeek Chat',
          quantizations: [],
          vision: false,
          reasoning: true,
        },
      ],
    })
    const save = vi.spyOn(commands, 'savePreferences').mockResolvedValue(configured)
    render(
      <ThemeProvider attribute='class'>
        <SettingsPage />
      </ThemeProvider>,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Translation' }))
    const reasoning = screen.getByRole('switch', { name: 'Enable reasoning' })
    expect(reasoning).not.toHaveAttribute('aria-disabled', 'true')
    expect(screen.getByRole('switch', { name: 'Vision' })).not.toHaveAttribute(
      'aria-disabled',
      'true',
    )
    await user.click(reasoning)
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        expect.objectContaining({
          translation: expect.objectContaining({
            generation: expect.objectContaining({ reasoning: true }),
          }),
        }),
        configured.providers,
        configured.typesetting,
      ),
    )
  })

  it('keeps a configured provider credential private and allows clearing it', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    const providers = {
      entries: [
        {
          name: 'DeepL',
          config: { provider: 'deepl' as const, settings: { base_url: null } },
          credential: { configured: true, value: null, clear: false },
        },
      ],
    }
    render(<ProviderPreferences value={providers} onChange={onChange} />)

    const input = screen.getByLabelText('DeepL credential')
    expect(screen.getByText('Credential')).toHaveAttribute('for', input.id)
    expect(input).toHaveAttribute('type', 'text')
    expect(input).toHaveAttribute('autocomplete', 'off')
    expect(input).toHaveAttribute('autocapitalize', 'none')
    expect(input).toHaveAttribute('spellcheck', 'false')
    expect(input).toHaveClass(
      '[-webkit-text-security:disc]',
      '[&::placeholder]:[-webkit-text-security:none]',
    )
    expect(input).toHaveValue('')
    expect(input).toHaveAttribute('placeholder', 'Configured')
    expect(
      screen.queryByRole('button', { name: 'Reveal DeepL credential' }),
    ).not.toBeInTheDocument()

    const clear = screen.getByRole('button', { name: 'Clear DeepL credential' })
    expect(clear.querySelector('.lucide-eraser')).toBeInTheDocument()
    expect(clear).not.toHaveClass('text-destructive')
    await user.click(clear)
    expect(onChange).toHaveBeenCalledWith({
      entries: [
        expect.objectContaining({
          credential: { configured: false, value: null, clear: true },
        }),
      ],
    })
  })

  it('preserves a credential draft and focus when autosave finishes', async () => {
    installProject()
    const user = userEvent.setup()
    useKoharuStore.setState({ settingsOpen: true })
    const save = vi
      .spyOn(commands, 'savePreferences')
      .mockImplementation(async (pipeline, providers, typesetting) => ({
        ...preferences,
        pipeline,
        providers: {
          entries: providers.entries.map((entry) => ({
            ...entry,
            credential: entry.credential?.value
              ? { configured: true, value: null, clear: false }
              : entry.credential,
          })),
        },
        typesetting,
      }))
    render(
      <ThemeProvider attribute='class'>
        <SettingsPage />
      </ThemeProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Providers' }))
    const input = screen.getByLabelText('DeepL credential')
    await user.type(input, 's')
    await waitFor(() => expect(save).toHaveBeenCalled())

    await waitFor(() => {
      expect(screen.getByLabelText('DeepL credential')).toBe(input)
      expect(input).toHaveFocus()
      expect(input).toHaveValue('s')
    })
  })

  it('saves and clears the default color only when detection is disabled', async () => {
    installProject()
    const user = userEvent.setup()
    useKoharuStore.setState({ settingsOpen: true })
    const existingColor = [...textLayer.typography!.color!]
    const save = vi.spyOn(commands, 'savePreferences')
      .mockImplementation(async (pipeline, providers, typesetting) => ({
        ...preferences, pipeline, providers, typesetting,
      }))
    const setTypography = vi.spyOn(commands, 'setTypography').mockResolvedValue(null)
    render(<ThemeProvider attribute='class'><SettingsPage /></ThemeProvider>)

    await user.click(screen.getByRole('button', { name: 'Pipeline' }))
    const toggle = screen.getByRole('switch', { name: 'Detect text and outline colors' })
    const color = screen.getByRole('button', { name: 'Default text color' })
    expect(toggle).toBeChecked()
    expect(color).toBeDisabled()
    await user.click(toggle)
    expect(color).toBeEnabled()
    await user.click(color)
    fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
      target: { value: '#2357AB' },
    })
    await waitFor(() => expect(save).toHaveBeenLastCalledWith(
      expect.objectContaining({ detect_text_color: false, default_text_color: [35, 87, 171, 255] }),
      preferences.providers, preferences.typesetting,
    ))
    await user.click(screen.getByRole('button', { name: 'Transparent' }))
    await waitFor(() => expect(save).toHaveBeenLastCalledWith(
      expect.objectContaining({ detect_text_color: false, default_text_color: null }),
      preferences.providers, preferences.typesetting,
    ))
    expect(setTypography).not.toHaveBeenCalled()
    expect(textLayer.typography!.color).toEqual(existingColor)
  })

  it('clamps a threshold typed past its bounds before saving it', async () => {
    installProject()
    useKoharuStore.setState({ settingsOpen: true })
    const save = vi.spyOn(commands, 'savePreferences').mockResolvedValue(preferences)
    render(
      <ThemeProvider attribute='class'>
        <SettingsPage />
      </ThemeProvider>,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Pipeline' }))
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Text threshold' }), {
      target: { value: '15' },
    })
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        expect.objectContaining({
          processor: expect.objectContaining({
            'koharu-layout-rfdetr-seg-2xl': expect.objectContaining({ text_threshold: 1 }),
          }),
        }),
        preferences.providers,
        preferences.typesetting,
      ),
    )
  })

  it('saves a newer OpenRouter selection after an in-flight provider save', async () => {
    installProject()
    const user = userEvent.setup()
    const configured: Preferences = {
      ...preferences,
      providers: {
        entries: [
          ...preferences.providers.entries,
          {
            name: 'OpenRouter',
            config: { provider: 'openrouter', settings: {} },
            credential: emptyCredential(),
          },
        ],
      },
    }
    useKoharuStore.setState({
      settingsOpen: true,
      preferences: configured,
      translationModels: [
        ...useKoharuStore.getState().translationModels,
        {
          provider: 'openrouter',
          model: 'openrouter/auto',
          name: 'OpenRouter Auto',
          quantizations: [],
          vision: true,
          reasoning: true,
        },
      ],
    })
    let resolveFirst: ((value: Preferences) => void) | undefined
    const first = new Promise<Preferences>((resolve) => {
      resolveFirst = resolve
    })
    let firstResult: Preferences | undefined
    let invocation = 0
    const save = vi
      .spyOn(commands, 'savePreferences')
      .mockImplementation(async (pipeline, providers, typesetting) => {
        const saved = { ...configured, pipeline, providers, typesetting }
        if (invocation++ === 0) {
          firstResult = saved
          return first
        }
        return saved
      })
    render(
      <ThemeProvider attribute='class'>
        <SettingsPage />
      </ThemeProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Providers' }))
    await user.type(screen.getByLabelText('OpenRouter credential'), 'secret')
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1))
    await user.click(screen.getByRole('button', { name: 'Translation' }))
    await user.click(screen.getByLabelText('Translation model'))
    await user.click(screen.getByRole('button', { name: 'Use OpenRouter Auto from OpenRouter' }))
    await user.click(screen.getByRole('button', { name: 'Back to editor' }))

    expect(save).toHaveBeenCalledTimes(1)
    await act(async () => {
      resolveFirst?.(firstResult!)
      await first
    })
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2))
    await waitFor(() => {
      expect(useKoharuStore.getState().settingsOpen).toBe(false)
      expect(useKoharuStore.getState().preferences?.pipeline.translation.model).toMatchObject({
        provider: 'openrouter',
        model: 'openrouter/auto',
      })
    })
  })

  it('adds and removes default font families from typesetting settings', async () => {
    installProject()
    const user = userEvent.setup()
    useKoharuStore.setState({ settingsOpen: true })
    queryClient.setQueryData(fontsKey, [
      {
        name: 'Noto Sans',
        metadata: {
          primary_script: 'latn',
          scripts: ['latn'],
          languages: ['en'],
          category: 'SANS_SERIF',
          classifications: ['sans-serif'],
          use_cases: ['body-text'],
        },
        sources: ['system'],
        faces: [
          {
            postscript_name: 'NotoSans-Regular',
            weight: 400,
            weight_range: null,
            style: 'normal',
          },
        ],
      },
      {
        name: 'Arial',
        metadata: {
          primary_script: 'latn',
          scripts: ['latn'],
          languages: ['en'],
          category: 'SANS_SERIF',
          classifications: ['sans-serif'],
          use_cases: ['body-text'],
        },
        sources: ['system'],
        faces: [
          {
            postscript_name: 'ArialMT',
            weight: 400,
            weight_range: null,
            style: 'normal',
          },
        ],
      },
    ])
    const save = vi
      .spyOn(commands, 'savePreferences')
      .mockImplementation(async (pipeline, providers, typesetting) => ({
        ...preferences,
        pipeline,
        providers,
        typesetting,
      }))
    render(
      <ThemeProvider attribute='class'>
        <SettingsPage />
      </ThemeProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Typesetting' }))
    expect(screen.getByRole('heading', { level: 2, name: 'Typesetting' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Add font family' }))
    await user.click(await screen.findByRole('option', { name: 'Arial, System' }))

    await waitFor(() =>
      expect(save).toHaveBeenLastCalledWith(preferences.pipeline, preferences.providers, {
        font_families: ['Noto Sans', 'Arial'],
      }),
    )
    await user.click(screen.getByRole('button', { name: 'Remove Noto Sans' }))
    await waitFor(() =>
      expect(save).toHaveBeenLastCalledWith(preferences.pipeline, preferences.providers, {
        font_families: ['Arial'],
      }),
    )
  })

  it('shows model resources in the left sidebar footer', () => {
    useKoharuStore.setState({
      resources: {
        process_memory: 2 * 1024 ** 3,
        system_memory: 64 * 1024 ** 3,
        process_cpu: 8,
        devices: [
          {
            name: 'GPU',
            selected: true,
            memory_budget: 16 * 1024 ** 3,
            memory_used: 6 * 1024 ** 3,
            utilization: 40,
          },
        ],
      },
    })
    render(<ResourceMonitor />)
    expect(screen.getByText('8%')).toBeInTheDocument()
    expect(screen.getByText('3%')).toBeInTheDocument()
  })

  it('unloads from the resource monitor and blocks duplicate or running-job requests', async () => {
    const user = userEvent.setup()
    useKoharuStore.setState({ resources: {
      process_memory: 1024, system_memory: 4096, process_cpu: 8, devices: [],
    } })
    const first = Promise.withResolvers<number>()
    const unload = vi.spyOn(commands, 'unloadModels')
      .mockReturnValueOnce(first.promise).mockResolvedValue(0)
    const notify = vi.spyOn(toast, 'add')
    render(<ResourceMonitor />)
    await user.click(screen.getByText('8%').closest('button')!)
    const button = screen.getByRole('button', { name: 'Unload models' })
    await user.click(button)
    expect(button).toBeDisabled()
    await user.click(button)
    expect(unload).toHaveBeenCalledTimes(1)
    await act(async () => first.resolve(3))
    await waitFor(() => expect(button).toBeEnabled())
    expect(notify).toHaveBeenLastCalledWith(expect.objectContaining({ type: 'success' }))
    await user.click(button)
    await waitFor(() => expect(notify).toHaveBeenLastCalledWith(expect.objectContaining({ type: 'info' })))
    act(() => useKoharuStore.setState({ jobs: {
      job: { id: 'job', state: 'running', completed: 0, total: 1, page: null,
        stage: null, model: null, error: null },
    } }))
    expect(button).toBeDisabled()
    await user.click(button)
    expect(unload).toHaveBeenCalledTimes(2)
  })

  it('tracks arbitrary concurrent commands until each one settles', async () => {
    const first = Promise.withResolvers<string>()
    const second = Promise.withResolvers<string>()
    const command = vi
      .fn<(name: string, count: number) => Promise<string>>()
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise)
    const { result } = renderHook(
      () => useCommand(['rebuild-index'], command, 'Rebuilding index…'),
      {
        wrapper: ({ children }) => (
          <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
        ),
      },
    )
    render(<ActivityCenter />)

    act(() => {
      result.current.run('first', 2)
      result.current.run('second', 3)
    })
    await waitFor(() => expect(screen.getAllByRole('status')).toHaveLength(2))
    expect(screen.getAllByText('Rebuilding index…')).toHaveLength(2)
    expect(command).toHaveBeenNthCalledWith(1, 'first', 2)
    expect(command).toHaveBeenNthCalledWith(2, 'second', 3)
    expect(result.current.busy).toBe(true)

    await act(async () => first.resolve('done'))
    await waitFor(() => expect(screen.getAllByRole('status')).toHaveLength(1))
    expect(result.current.busy).toBe(true)

    await act(async () => second.reject(new Error('Index failed')))
    await waitFor(() => expect(screen.queryByRole('status')).not.toBeInTheDocument())
    expect(result.current.busy).toBe(false)
    expect(screen.queryByRole('complementary', { name: 'Activity' })).not.toBeInTheDocument()
  })

  it('keeps running work visible and stoppable', async () => {
    installProject()
    useKoharuStore.setState({
      jobs: {
        job: {
          state: 'running',
          id: 'job',
          completed: 1,
          total: 4,
          page: 'page',
          stage: 'ocr',
          model: 'manga-ocr',
          error: null,
        },
      },
    })
    // A real page label is a filename, which says nothing about position in
    // the run, so the row numbers it too.
    queryClient.setQueryData(pagesKey, [
      {
        id: 'page',
        label: 'cover.png',
        size: { width: 1000, height: 1500 },
        source_asset: 'source',
        layer_count: 1,
      },
    ])
    const stop = vi.spyOn(commands, 'stopJob').mockResolvedValue(null)
    render(<ActivityCenter />)
    expect(screen.getByText('25%')).toBeInTheDocument()
    // Separate elements, so a long label truncates without taking the model.
    expect(screen.getByText('Page 1: cover.png')).toBeInTheDocument()
    expect(screen.getByText('manga-ocr')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Stop' }))
    await waitFor(() => expect(stop).toHaveBeenCalledWith('job'))
  })

  it('combines concurrent downloads into one progress bar', () => {
    useKoharuStore.setState({
      downloads: {
        one: {
          id: 1,
          state: 'running',
          name: 'one.ttf',
          completed: 25,
          total: 100,
          error: null,
        },
        two: {
          id: 2,
          state: 'running',
          name: 'two.ttf',
          completed: 75,
          total: 100,
          error: null,
        },
      },
    })

    render(<ActivityCenter />)

    expect(screen.getByText('Downloading 2 files')).toBeInTheDocument()
    expect(screen.getByText('50%')).toBeInTheDocument()
    expect(screen.getAllByRole('progressbar')).toHaveLength(1)
    expect(screen.queryByText('one.ttf')).not.toBeInTheDocument()
    expect(screen.queryByText('two.ttf')).not.toBeInTheDocument()
  })
})

describe('font picker keyboard', () => {
  const familiesFor = (names: string[]): FontFamily[] =>
    names.map((name) => ({
      name,
      metadata: {
        primary_script: 'latn',
        scripts: ['latn'],
        languages: [],
        category: null,
        classifications: [],
        use_cases: [],
      },
      sources: ['system'],
      faces: [
        {
          postscript_name: name.replaceAll(' ', ''),
          weight: 400,
          weight_range: null,
          style: 'normal',
        },
      ],
    }))

  it('navigates, previews, and scrolls font choices with the keyboard', async () => {
    const user = userEvent.setup()
    const families = familiesFor(
      Array.from({ length: 32 }, (_, index) => `Font ${String(index + 1).padStart(2, '0')}`),
    )
    const getFontPreview = vi.spyOn(commands, 'getFontPreview').mockResolvedValue([1])
    vi.spyOn(commands, 'getFontFavorites').mockResolvedValue([])
    const onChange = vi.fn()
    render(<FontPicker value='Font 01' families={families} onChange={onChange} />)

    await user.click(screen.getByTestId('type-font-picker'))
    const search = screen.getByRole('combobox', { name: 'Search fonts' })
    const listbox = screen.getByRole('listbox', { name: 'Fonts' })
    expect(search).toHaveAttribute('aria-controls', listbox.id)
    const viewport = listbox.querySelector<HTMLElement>('[data-slot="scroll-area-viewport"]')
    expect(viewport).not.toBeNull()
    Object.defineProperties(viewport!, {
      clientHeight: { configurable: true, value: 248 },
      scrollHeight: { configurable: true, value: 1400 },
    })
    const scrollTo = vi.fn((options: ScrollToOptions) => {
      viewport!.scrollTop = options.top ?? 0
      viewport!.dispatchEvent(new Event('scroll'))
    })
    Object.defineProperty(viewport!, 'scrollTo', { configurable: true, value: scrollTo })

    for (let index = 0; index < 20; index += 1) fireEvent.keyDown(search, { key: 'ArrowDown' })

    const activeOptionId = search.getAttribute('aria-activedescendant')
    expect(activeOptionId).toBeTruthy()
    const activeOption = document.getElementById(activeOptionId!)
    expect(activeOption).toHaveTextContent('Font 21')
    expect(activeOption).toHaveAttribute('data-highlighted', 'true')
    await waitFor(() => expect(viewport!.scrollTop).toBeGreaterThan(0))
    await waitFor(() => expect(getFontPreview).toHaveBeenCalledWith('Font 21'))
    expect(onChange).not.toHaveBeenCalled()

    fireEvent.keyDown(search, { key: 'Enter' })
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Font 21')
    expect(screen.queryByRole('listbox', { name: 'Fonts' })).not.toBeInTheDocument()
  })

  it('follows the displayed favorites-first font order and skips headings', async () => {
    const user = userEvent.setup()
    vi.spyOn(commands, 'getFontFavorites').mockResolvedValue(['Zulu'])
    vi.spyOn(commands, 'getFontPreview').mockResolvedValue([])
    const onChange = vi.fn()
    render(
      <FontPicker value='' families={familiesFor(['Alpha', 'Beta', 'Zulu'])} onChange={onChange} />,
    )
    await user.click(screen.getByTestId('type-font-picker'))
    await screen.findByText('Favorites')
    const search = screen.getByRole('combobox', { name: 'Search fonts' })
    const active = () => document.getElementById(search.getAttribute('aria-activedescendant')!)

    expect(active()).toHaveTextContent('Zulu')
    fireEvent.keyDown(search, { key: 'ArrowUp' })
    expect(active()).toHaveTextContent('Zulu')
    fireEvent.keyDown(search, { key: 'ArrowDown' })
    expect(active()).toHaveTextContent('Alpha')
    fireEvent.keyDown(search, { key: 'ArrowDown' })
    expect(active()).toHaveTextContent('Beta')
    fireEvent.keyDown(search, { key: 'ArrowUp' })
    expect(active()).toHaveTextContent('Alpha')
    expect(onChange).not.toHaveBeenCalled()
    fireEvent.keyDown(search, { key: 'Enter' })
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Alpha')
  })

  it('resets the active font for search results and ignores an empty result set', async () => {
    const user = userEvent.setup()
    vi.spyOn(commands, 'getFontFavorites').mockResolvedValue([])
    vi.spyOn(commands, 'getFontPreview').mockResolvedValue([])
    const onChange = vi.fn()
    render(
      <FontPicker
        value='Zulu'
        families={familiesFor(['Alpha', 'Beta', 'Zulu'])}
        onChange={onChange}
      />,
    )
    await user.click(screen.getByTestId('type-font-picker'))
    const search = screen.getByRole('combobox', { name: 'Search fonts' })
    fireEvent.change(search, { target: { value: 'missing' } })
    fireEvent.keyDown(search, { key: 'ArrowDown' })
    fireEvent.keyDown(search, { key: 'Enter' })
    expect(search).not.toHaveAttribute('aria-activedescendant')
    expect(onChange).not.toHaveBeenCalled()
    fireEvent.change(search, { target: { value: 'Bet' } })
    expect(
      document.getElementById(search.getAttribute('aria-activedescendant')!),
    ).toHaveTextContent('Beta')
    fireEvent.keyDown(search, { key: 'Enter' })
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Beta')
  })

  it('keeps font browsing and Esc out of document history and applies Enter once', async () => {
    const user = userEvent.setup()
    installProject()
    useKoharuStore.setState({ inspector: 'type' })
    queryClient.setQueryData(fontsKey, familiesFor(['Noto Sans', 'Noto Serif']))
    vi.spyOn(commands, 'getFontFavorites').mockResolvedValue([])
    vi.spyOn(commands, 'getFontPreview').mockResolvedValue([])
    const setTypography = vi.spyOn(commands, 'setTypography').mockResolvedValue(null)
    render(<Inspector />)

    await user.click(screen.getByTestId('type-font-picker'))
    let search = screen.getByRole('combobox', { name: 'Search fonts' })
    fireEvent.keyDown(search, { key: 'ArrowDown' })
    expect(setTypography).not.toHaveBeenCalled()
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('listbox', { name: 'Fonts' })).not.toBeInTheDocument()
    expect(setTypography).not.toHaveBeenCalled()

    await user.click(screen.getByTestId('type-font-picker'))
    search = screen.getByRole('combobox', { name: 'Search fonts' })
    fireEvent.keyDown(search, { key: 'ArrowDown' })
    fireEvent.keyDown(search, { key: 'Enter' })
    await waitFor(() =>
      expect(setTypography).toHaveBeenCalledExactlyOnceWith([
        expect.objectContaining({
          layer: 'element',
          typography: expect.objectContaining({ preferred_font: 'Noto Serif' }),
        }),
      ]),
    )
  })
})
