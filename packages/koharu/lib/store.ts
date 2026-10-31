'use client'

import { create } from 'zustand'

import { soundForJobTransition } from '@/components/runs/sound'
import type {
  CanvasState,
  Download,
  EntityId,
  Job,
  Model,
  Frame,
  Preferences,
  Stage,
  StartupState,
  ModelResources,
  PageSize,
  WheelZoom,
  TextStyles,
} from '@koharu/bridge/protocol'
import { toast } from '@koharu/ui/components/toast'

import { isBinding } from './shortcuts'

export type CanvasTool =
  | 'select'
  | 'text'
  | 'draw'
  | 'eraser'
  | 'color_picker'
  | 'remove'
  | 'pan'
  | 'ocr_box'
export const MIN_BRUSH_DIAMETER = 1
export function maxBrushDiameter(size: PageSize): number {
  // Assume that for all sizes below 2048 width, 128 is sufficient.
  // For anything above, scale by percentage greater than 2048 width.
  return Math.round(Math.max(size.width / 2048, 1.0) * 128)
}

export function isBrushTool(tool: CanvasTool): boolean {
  return tool === 'draw' || tool === 'eraser' || tool === 'remove'
}

export interface CanvasBrush {
  diameter: number
  color: string
}
export type InspectorSection = 'copy' | 'type' | 'layers'
export type ShortcutAction =
  | CanvasTool
  | 'fit'
  | 'zoom_in'
  | 'zoom_out'
  | 'zoom_actual'
  | 'compare'
  | 'brush_smaller'
  | 'brush_larger'
export type ShortcutSlot = ShortcutAction | `${ShortcutAction}_alt`
export type Shortcuts = Record<ShortcutSlot, string>
export type PipelineScope = 'page' | 'selected-pages' | 'project'
export const pipelineStages: readonly Stage[] = ['detection', 'ocr', 'translation', 'inpainting']

interface KoharuStore {
  initialized: boolean
  textStyles: TextStyles | null
  preferences: Preferences | null
  translationModels: Model[]
  resources: ModelResources | null
  jobs: Record<string, Job>
  downloads: Record<string, Download>
  camera: { zoom: number; translation: [number, number]; fitted: boolean }
  canvasPage: EntityId | null
  canvasRevision: number | null
  canvasGeneration: number
  canvasSize: [number, number]
  fitCanvasRequest: number
  layerFrames: Record<EntityId, Frame>
  selectedLayers: EntityId[]
  selectedPages: EntityId[]
  tool: CanvasTool
  brush: CanvasBrush
  inspector: InspectorSection
  fontFavoritesOnly: boolean
  processingScope: PipelineScope
  processingStages: Stage[]
  settingsOpen: boolean
  shortcuts: Shortcuts
  wheelZoom: WheelZoom
  selectPages: (pages: EntityId[]) => void
  showInspector: (section: InspectorSection) => void
  setFontFavoritesOnly: (favoritesOnly: boolean) => void
  setProcessingScope: (scope: PipelineScope) => void
  setProcessingStages: (stages: Stage[]) => void
  setSettingsOpen: (open: boolean) => void
  selectLayers: (layers: EntityId[]) => void
  setTool: (tool: CanvasTool) => void
  setBrush: (brush: CanvasBrush) => void
  setShortcut: (slot: ShortcutSlot, binding: string) => void
  setShortcuts: (shortcuts: Shortcuts) => void
  setWheelZoom: (wheelZoom: WheelZoom) => void
  requestCanvasFit: () => void
  dismissJob: (id: string) => void
  dismissDownload: (id: number) => void
}

export const defaultShortcuts: Shortcuts = {
  select: 'v',
  text: 't',
  draw: 'b',
  eraser: 'e',
  color_picker: 'i',
  remove: 'j',
  pan: 'h',
  fit: '0',
  zoom_in: 'ctrl+plus',
  zoom_out: 'ctrl+-',
  zoom_actual: 'ctrl+0',
  compare: '\\',
  ocr_box: 'o',
  brush_smaller: '[',
  brush_larger: ']',
  select_alt: '',
  text_alt: '',
  draw_alt: '',
  eraser_alt: '',
  color_picker_alt: '',
  remove_alt: '',
  pan_alt: '',
  fit_alt: '',
  zoom_in_alt: 'ctrl+numpadadd',
  zoom_out_alt: 'ctrl+numpadsubtract',
  zoom_actual_alt: '',
  compare_alt: '',
  ocr_box_alt: '',
  brush_smaller_alt: '',
  brush_larger_alt: '',
}

export const useKoharuStore = create<KoharuStore>()((set) => ({
  initialized: false,
  textStyles: null,
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
  fontFavoritesOnly: false,
  processingScope: 'selected-pages',
  processingStages: [...pipelineStages],
  settingsOpen: false,
  shortcuts: defaultShortcuts,
  wheelZoom: 'ctrl',
  selectPages: (selectedPages) => set({ selectedPages: [...new Set(selectedPages)] }),
  showInspector: (inspector) => set({ inspector }),
  setFontFavoritesOnly: (fontFavoritesOnly) => set({ fontFavoritesOnly }),
  setProcessingScope: (processingScope) => set({ processingScope }),
  setProcessingStages: (processingStages) => set({ processingStages }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  selectLayers: (selectedLayers) => set({ selectedLayers: [...new Set(selectedLayers)] }),
  setTool: (tool) => set({ tool }),
  setBrush: (brush) => set({ brush }),
  setShortcut: (slot, binding) =>
    set((state) => ({
      shortcuts: { ...state.shortcuts, [slot]: binding },
    })),
  setShortcuts: (shortcuts) => set({ shortcuts: { ...shortcuts } }),
  setWheelZoom: (wheelZoom) => set({ wheelZoom }),
  requestCanvasFit: () => set((state) => ({ fitCanvasRequest: state.fitCanvasRequest + 1 })),
  dismissJob: (id) =>
    set((state) => {
      const jobs = { ...state.jobs }
      delete jobs[id]
      return { jobs }
    }),
  dismissDownload: (id) =>
    set((state) => {
      const downloads = { ...state.downloads }
      delete downloads[String(id)]
      return { downloads }
    }),
}))

export function receiveStartupState(state: StartupState): void {
  useKoharuStore.setState({
    initialized: true,
    preferences: state.preferences,
    jobs: byId(state.jobs),
    ...canvasSnapshot(state.canvas),
  })
}

/** Apply saved shortcut keys over the defaults, ignoring actions the editor no longer has. */
export function receiveShortcuts(saved: Partial<{ [key in string]: string }>): void {
  const shortcuts = { ...defaultShortcuts }
  for (const slot of Object.keys(defaultShortcuts) as ShortcutSlot[]) {
    const binding = saved[slot]
    if (typeof binding === 'string' && isBinding(binding)) shortcuts[slot] = binding
  }
  useKoharuStore.setState({ shortcuts })
}

export function receiveCanvas(canvas: CanvasState): void {
  useKoharuStore.setState(canvasSnapshot(canvas))
}

export function receiveJob(job: Job): void {
  soundForJobTransition(useKoharuStore.getState().jobs[job.id], job)
  useKoharuStore.setState((state) => ({ jobs: { ...state.jobs, [job.id]: job } }))
}

export function receiveDownload(download: Download): void {
  useKoharuStore.setState((state) => {
    const downloads = { ...state.downloads }
    if (download.state === 'finished') delete downloads[String(download.id)]
    else downloads[String(download.id)] = download
    return { downloads }
  })
}

export function receivePreferences(preferences: Preferences): void {
  useKoharuStore.setState({ preferences })
}

export function receiveTranslationModels(translationModels: Model[]): void {
  useKoharuStore.setState({ translationModels })
}

export function receiveResources(resources: ModelResources): void {
  useKoharuStore.setState({ resources })
}

export function receiveError(message: string): void {
  toast.add({ type: 'error', title: 'Could not complete that action', description: message })
}

function canvasSnapshot(canvas: CanvasState) {
  return {
    canvasPage: canvas.page,
    canvasRevision: canvas.revision,
    canvasGeneration: canvas.generation,
    canvasSize: canvas.size,
    layerFrames: Object.fromEntries(
      canvas.element_frames.map(({ element, frame }) => [element, frame]),
    ),
  }
}

function byId<T extends { id: string | number }>(items: T[]): Record<string, T> {
  return Object.fromEntries(items.map((item) => [String(item.id), item]))
}
