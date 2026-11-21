'use client'

import { useQuery } from '@tanstack/react-query'
import { create } from 'zustand'

import { booharuPreferencesQuery, saveBooharuSection } from '@/lib/booharu/preferences'
import {
  modelsKey,
  preferencesKey,
  projectKey,
  queryClient,
  useModels,
  usePreferences,
  type ProjectView,
} from '@/lib/queries'
import {
  commands,
  type BooharuPreferences,
  type CantRead,
  type ChapterState,
  type Model,
  type ReadingSettings,
} from '@koharu/ipc/protocol'

/** `[booharu-reading]` with every value, as the desktop answers it. */
export type Reading = Required<ReadingSettings>

/** The most pages one Read chapter request sends, as booharu-core checks it. */
export const MAX_PAGES_PER_REQUEST = 40

/** The id of the Read chapter model's picker in Settings > Pipeline, which a link goes to. */
export const readingSection = 'booharu-reading-model'

/** The section while the file sets nothing, as booharu-core's defaults are. */
const builtInReading: Reading = { model: null, ask: true, pages_per_request: 20 }

/** `section` with every value: one the desktop leaves out is the built-in one, as booharu-core
 * reads a missing key. */
export function completeReading(section: ReadingSettings | undefined): Reading {
  return {
    model: section?.model ?? builtInReading.model,
    ask: section?.ask ?? builtInReading.ask,
    pages_per_request: section?.pages_per_request ?? builtInReading.pages_per_request,
  }
}

/** Whether `pages` is a number of pages one request takes: a whole number from 1 to 40. */
export function isPagesPerRequest(pages: number | null): pages is number {
  return pages !== null && Number.isInteger(pages) && pages >= 1 && pages <= MAX_PAGES_PER_REQUEST
}

/** Saves `change` over `[booharu-reading]` as the file holds it just before the save, with every
 * other section as the file holds it then, and answers the section as saved. A failure shows as
 * every failed command's does. */
export async function saveReading(change: Partial<Reading>): Promise<Reading> {
  const saved = await saveBooharuSection('booharu-reading', (section) => ({
    ...completeReading(section),
    ...change,
  }))
  return completeReading(saved['booharu-reading'])
}

const selectReading = (preferences: BooharuPreferences) =>
  completeReading(preferences['booharu-reading'])

/** `[booharu-reading]` as the settings file holds it, shared with the settings pages and refreshed
 * by every save: read again each time a view showing it opens, as the agent's settings tool may
 * have changed it since. A read that failed has shown its error, and isn't tried again each time a
 * view needing it opens. Undefined until the first read ends. */
export function useReadingSettings(): Reading | undefined {
  return useQuery({
    ...booharuPreferencesQuery,
    select: selectReading,
    refetchOnMount: 'always',
    retryOnMount: false,
  }).data
}

/** The vendor of Koharu's own models, which run on this computer. */
const KOHARU = 'koharu'

/** Whether `reference` names one of Koharu's own models, which reads on this computer rather than
 * through a provider's API. */
export function isLocal(reference: string): boolean {
  const at = reference.indexOf('/')
  return at >= 0 && reference.slice(0, at) === KOHARU
}

/** `reference` without the quantization one of Koharu's own may name after a colon; another
 * vendor's id may hold a colon of its own, as Ollama's tags do. */
function unquantized(reference: string): string {
  return isLocal(reference) ? (reference.split(':', 1)[0] ?? reference) : reference
}

/** The model of `models` that `reference` names; for one of Koharu's own, whatever quantization it
 * names. */
export function listedModel(models: readonly Model[], reference: string): Model | undefined {
  const wanted = unquantized(reference)
  return models.find((model) => model.reference === wanted)
}

/** Whether `model` can read a chapter's pages: it sees images and calls tools, a general model,
 * as booharu's model rules have it. Koharu's default translator sees images but only translates. */
export function canRead(model: Model): boolean {
  return model.images && model.tools
}

/** Why the model `reference` names can't read a chapter's pages, as `models` lists it; null when
 * it can, or when the list doesn't have it, which counts as able, as Koharu counts a model it
 * doesn't know. */
export function cantRead(models: readonly Model[], reference: string): CantRead | null {
  const model = listedModel(models, reference)
  if (!model) return null
  if (!model.images) return 'no_vision'
  return model.tools ? null : 'only_translates'
}

/** The model a read would use, as the desktop picks it: `saved`, the Read chapter model, when it
 * can read; with none saved, `translation`, the translation model, when it can; else none. A saved
 * model that can't read makes a read refuse rather than fall back, so it gives none. */
export function readerOf(
  models: readonly Model[],
  saved: string | null,
  translation: string | null,
): string | null {
  if (saved) return cantRead(models, saved) ? null : saved
  return translation && !cantRead(models, translation) ? translation : null
}

/** The model a read would use now, as `readerOf` picks it from Koharu's list of models, the
 * translation model and the saved Read chapter model. */
export function useReader(): string | null {
  const models = useModels().data ?? []
  const translation = usePreferences().data?.pipeline.translator?.model ?? null
  const saved = useReadingSettings()?.model ?? null
  return readerOf(models, saved, translation)
}

/** Whether the chapter's brief is missing: it has none, or its summary is blank, which counts as
 * none, so a Run press reads the chapter again. */
export function briefMissing(chapter: ChapterState): boolean {
  return !chapter.context.brief?.summary?.trim()
}

/** Whether the Run menu's Read chapter starts on for `chapter`, the open project's: it links a
 * series that reads, its brief is missing, and `reader`, the model a read would use, is one. */
export function readsByDefault(
  chapter: ChapterState | null | undefined,
  reader: string | null,
): boolean {
  return Boolean(chapter && chapter.series !== null && briefMissing(chapter) && reader !== null)
}

/** The Run menu's Read chapter switch, by project, while the app runs, as Koharu's own switches
 * aren't saved either: a project shows none until the user switches it, or applies a preset, and
 * follows its default till then. */
export const useReadSwitches = create<{ on: Record<string, boolean> }>()(() => ({ on: {} }))

/** Switches Read chapter on or off in the Run menu of the project `project`. */
export function setReadChapter(project: string, on: boolean): void {
  useReadSwitches.setState((state) => ({ on: { ...state.on, [project]: on } }))
}

/** The open project's name, none while none is open. */
export function openProject(): string | null {
  return queryClient.getQueryData<ProjectView | null>(projectKey)?.name ?? null
}

/** Whether a Run press reads the chapter first, as the Run menu's Read chapter shows: as switched
 * in the open project, else its default, from the chapter as the desktop reads it now, the
 * settings as last read, read now when none were, and the models as last listed, none when they
 * weren't, as listing them can take a while. A read that fails reads nothing. The run settings
 * load this before the chapter's queries can, so it asks the desktop itself. */
export async function readChapterOn(): Promise<boolean> {
  const project = openProject()
  if (project === null) return false
  const switched = useReadSwitches.getState().on[project]
  if (switched !== undefined) return switched
  try {
    const [chapter, preferences, koharu] = await Promise.all([
      commands.booharuGetChapterContext(),
      queryClient.fetchQuery(booharuPreferencesQuery),
      queryClient.ensureQueryData({
        queryKey: preferencesKey,
        queryFn: () => commands.getPreferences(),
      }),
    ])
    const models = queryClient.getQueryData<Model[]>(modelsKey) ?? []
    const saved = completeReading(preferences['booharu-reading']).model
    const translation = koharu.pipeline.translator?.model ?? null
    return readsByDefault(chapter, readerOf(models, saved, translation))
  } catch {
    return false
  }
}
