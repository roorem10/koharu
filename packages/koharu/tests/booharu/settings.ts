import { vi } from 'vitest'

import {
  commands,
  type BooharuPreferences,
  type LetteringSettings,
  type ReadingSettings,
  type RunsSettings,
} from '@koharu/ipc/protocol'

/** `[booharu-lettering]` as the desktop answers it while the file sets nothing. */
export const builtInLettering = {
  default: { fill: null, stroke: null, stroke_width: 1.5 },
  detect_colors: true,
  auto: false,
  categories: {},
  small_text: 0.012,
  model_confidence: 0.6,
} satisfies LetteringSettings

/** `[booharu-runs]` as the desktop answers it while the file sets nothing. */
export const builtInRuns = {
  presets: [
    { name: 'Detect + OCR', stages: ['detect', 'recognize'] },
    { name: 'Translate + clean up', stages: ['inpaint', 'translate'] },
  ],
  retries: 2,
  on_page_failure: 'continue',
  order: 'pages',
  finish_sound: true,
} satisfies RunsSettings

/** `[booharu-reading]` as the desktop answers it while the file sets nothing. */
export const builtInReading = {
  model: null,
  ask: true,
  pages_per_request: 20,
} satisfies ReadingSettings

/** The desktop with every command answering null, and the Booharu settings its file holds, which
 * another writer, such as the agent's settings tool, may change while a page shows them; `saves`
 * lists what each save sent. */
export function settingsDesktop(lettering: LetteringSettings = builtInLettering) {
  const table = commands as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>
  for (const name of Object.keys(table)) vi.spyOn(table, name).mockResolvedValue(null)
  const file: { preferences: BooharuPreferences } = {
    preferences: {
      'booharu-mcp': { port: 47821 },
      'booharu-ocr-box': { default_mode: 'read' },
      'booharu-lettering': structuredClone(lettering),
      'booharu-fonts': { favorites: [] },
      'booharu-runs': structuredClone(builtInRuns),
      'booharu-reading': structuredClone(builtInReading),
      'booharu-qc': { min_size: 0.01 },
      'booharu-start': { fresh_start_note_seen: false },
    },
  }
  const saves: BooharuPreferences[] = []
  vi.spyOn(commands, 'booharuGetPreferences').mockImplementation(async () =>
    structuredClone(file.preferences),
  )
  const save = vi
    .spyOn(commands, 'booharuSavePreferences')
    .mockImplementation(async (preferences) => {
      saves.push(structuredClone(preferences))
      file.preferences = structuredClone(preferences)
      return structuredClone(file.preferences)
    })
  vi.spyOn(commands, 'listFonts').mockResolvedValue([])
  return { file, saves, save }
}

/** Lets a debounced save that would come, come: longer than the settings' autosave delay. */
export function settle(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 400))
}
