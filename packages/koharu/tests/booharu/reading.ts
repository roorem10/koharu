import { vi } from 'vitest'

import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type ChapterBrief,
  type ChapterState,
  type Model,
  type Preferences,
  type Series,
} from '@koharu/ipc/protocol'

import { blueLock } from './context'
import { settingsDesktop } from './settings'

/** A hosted model that sees images and calls tools, which can read a chapter. */
export const gpt: Model = {
  reference: 'openai/gpt-5.4',
  name: 'GPT-5.4',
  vendor: 'openai',
  images: true,
  tools: true,
}

/** One of Koharu's own models that can read a chapter, on this computer. */
export const gemma: Model = {
  reference: 'koharu/gemma4-12b-it',
  name: 'Gemma 4 12B',
  vendor: 'koharu',
  images: true,
  tools: true,
}

/** Koharu's default translator, which sees images but only translates. */
export const indexTranslate: Model = {
  reference: 'koharu/index-translate-9b',
  name: 'Index-Translate 9B',
  vendor: 'koharu',
  images: true,
  tools: false,
}

/** A hosted model that sees no images. */
export const textOnly: Model = {
  reference: 'openai/text-only',
  name: 'Text Only',
  vendor: 'openai',
  images: false,
  tools: true,
}

/** The models the user can pick from, as Koharu lists them. */
export const models = [gpt, gemma, indexTranslate, textOnly]

/** The open project's chapter, linking `series`, with `brief`. */
export function chapterState(
  brief: ChapterBrief | null = null,
  series: Series | null = blueLock(),
): ChapterState {
  return {
    context: { version: 1, series: series?.id ?? null, brief, suggestions: [] },
    series,
    missing: false,
    unreadable: null,
    glossary: [],
    fresh: false,
    left_out: 0,
  }
}

/** Koharu's preferences, translating with `translation`. */
export function koharuPreferences(translation: string): Preferences {
  return {
    pipeline: { translator: { model: translation, target: 'en' } },
    renderer: { font_families: [] },
    agent: {},
    providers: {},
  }
}

/** Gives every element the size of Koharu's model menu, as jsdom lays nothing out: the menu shows
 * only the rows in view of its list. */
export function layOutModelMenus(): void {
  vi.spyOn(Element.prototype, 'clientHeight', 'get').mockReturnValue(288)
  vi.spyOn(Element.prototype, 'clientWidth', 'get').mockReturnValue(256)
}

/** The desktop as `settingsDesktop` has it, translating with `translation`, the Read chapter model
 * `saved`, and the open project's chapter `chapter`, with `models` to pick from, which Koharu's
 * model menu shows. */
export function readingDesktop({
  translation = gpt.reference,
  saved = null as string | null,
  chapter = chapterState(),
}: { translation?: string; saved?: string | null; chapter?: ChapterState } = {}) {
  const desktop = settingsDesktop()
  desktop.file.preferences['booharu-reading'].model = saved
  const preferences = koharuPreferences(translation)
  vi.spyOn(commands, 'listModels').mockResolvedValue(models)
  vi.spyOn(commands, 'getPreferences').mockResolvedValue(preferences)
  const read = vi
    .spyOn(commands, 'booharuGetChapterContext')
    .mockImplementation(async () => structuredClone(chapter))
  useKoharuStore.setState({ models, preferences })
  layOutModelMenus()
  return { ...desktop, read }
}
