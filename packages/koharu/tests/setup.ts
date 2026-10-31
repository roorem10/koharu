import '@testing-library/jest-dom/vitest'
import { afterEach, beforeEach, vi } from 'vitest'

import { queryClient } from '@/lib/queries'
import { defaultShortcuts, useKoharuStore } from '@/lib/store'
import { emptyTextStyle, emptyShapeStyles, emptyCategoryStyles } from '@/lib/text-styles'
import { commands } from '@koharu/bridge/protocol'

import { mockWo06 } from './mocks/wo06'
import { mockWo07 } from './mocks/wo07'
import { mockWo10 } from './mocks/wo10'
import { mockWo11a } from './mocks/wo11a'
import { mockWo11b } from './mocks/wo11b'
import { mockWo11c } from './mocks/wo11c'
import { mockWo11d } from './mocks/wo11d'
import { mockWo11e } from './mocks/wo11e'
import { mockWo11f } from './mocks/wo11f'
import { mockWo12 } from './mocks/wo12'
import { mockWo13 } from './mocks/wo13'
import { mockWo14 } from './mocks/wo14'
import { mockWo15 } from './mocks/wo15'
import { mockWo17 } from './mocks/wo17'
import { mockWo20 } from './mocks/wo20'
import { mockWo21 } from './mocks/wo21'
import { mockWo23 } from './mocks/wo23'
import { mockWo24 } from './mocks/wo24'

vi.mock('react-i18next', async (importOriginal) => {
  const actual = await importOriginal<typeof import('react-i18next')>()
  const { createInstance } = await import('i18next')
  const { default: translations } = await import('@/public/locales/en-US/translation.json')
  const i18n = createInstance()
  await i18n.init({
    resources: { 'en-US': { translation: translations } },
    lng: 'en-US',
    fallbackLng: 'en-US',
    interpolation: { escapeValue: false },
  })

  return {
    ...actual,
    useTranslation: () => ({ t: i18n.t.bind(i18n), i18n }),
    initReactI18next: { type: '3rdParty', init: () => undefined },
    I18nextProvider: ({ children }: { children: React.ReactNode }) => children,
  }
})

vi.mock('@tauri-apps/api/window', () => {
  const window = {
    close: vi.fn(async () => undefined),
    isMaximized: vi.fn(async () => false),
    minimize: vi.fn(async () => undefined),
    onResized: vi.fn(async () => () => undefined),
    startResizeDragging: vi.fn(async () => undefined),
    toggleMaximize: vi.fn(async () => undefined),
  }
  return { getCurrentWindow: () => window }
})

class Observer {
  observe() {}
  unobserve() {}
  disconnect() {}
}

Object.defineProperty(globalThis, 'ResizeObserver', { value: Observer, writable: true })
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: vi.fn((query: string) => ({
    matches: false,
    media: query,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
  })),
})
Element.prototype.scrollIntoView = vi.fn()
Element.prototype.scrollTo = vi.fn()
Element.prototype.setPointerCapture = vi.fn()
Element.prototype.hasPointerCapture = vi.fn(() => true)
Element.prototype.releasePointerCapture = vi.fn()
Element.prototype.getAnimations = vi.fn(() => [])
Object.defineProperty(URL, 'createObjectURL', {
  value: vi.fn(() => 'blob:koharu-thumbnail'),
  writable: true,
})
Object.defineProperty(URL, 'revokeObjectURL', { value: vi.fn(), writable: true })

const initial = useKoharuStore.getState()
beforeEach(() => {
  vi.spyOn(commands, 'getTextBubbleShape').mockResolvedValue({
    bubble: null,
    shape: null,
    measures: null,
  })
  vi.spyOn(commands, 'getChapterContext').mockResolvedValue({
    context: { version: 1, series: null, brief: null, suggestions: [], lines: {} },
    series: null,
    supports_tasks: false,
  })
  vi.spyOn(commands, 'getContextSettings').mockResolvedValue({
    read_when: 'missing',
    send_images: true,
    pages_per_request: 20,
    previous_pages: 1,
    agent_messages: 20,
  })
  vi.spyOn(commands, 'listSeries').mockResolvedValue([])
  vi.spyOn(commands, 'getAgentHistory').mockResolvedValue({ messages: [], running: null })
  vi.spyOn(commands, 'getLineCategory').mockResolvedValue(null)
  vi.spyOn(commands, 'getEffectiveCategory').mockResolvedValue({
    category: 'floating',
    reason: 'bubble_shape',
  })
  vi.spyOn(commands, 'getLetteringSettings').mockResolvedValue({
    sfx: 'skip',
    model_confidence: 0.6,
    small_text_threshold: 0.012,
  })
  mockWo20()
  mockWo12()
  mockWo13()
  mockWo14()
  mockWo06()
  mockWo21()
  mockWo07()
  mockWo15()
  mockWo10()
  mockWo17()
  mockWo23()
  mockWo24()
  mockWo11a()
  mockWo11b()
  mockWo11c()
  mockWo11d()
  mockWo11e()
  mockWo11f()
  queryClient.clear()
  useKoharuStore.setState({
    ...initial,
    initialized: false,
    textStyles: {
      default_style: { ...emptyTextStyle },
      shape_styles: structuredClone(emptyShapeStyles),
      category_styles: structuredClone(emptyCategoryStyles),
      legacy_color_migrated: true,
    },
    preferences: null,
    translationModels: [],
    resources: null,
    jobs: {},
    downloads: {},
    camera: { zoom: 1, translation: [0, 0], fitted: true },
    canvasPage: null,
    canvasRevision: null,
    canvasGeneration: 0,
    canvasSize: [0, 0],
    fitCanvasRequest: 0,
    layerFrames: {},
    selectedLayers: [],
    selectedPages: [],
    tool: 'select',
    brush: { diameter: 48, color: '#FFFFFF' },
    inspector: 'copy',
    processingScope: 'selected-pages',
    processingStages: ['detection', 'ocr', 'translation', 'inpainting'],
    settingsOpen: false,
    shortcuts: defaultShortcuts,
  })
})

afterEach(() => {
  vi.restoreAllMocks()
})
