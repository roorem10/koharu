import { vi } from 'vitest'

import { projectKey, queryClient, type ProjectView } from '@/lib/queries'
import { commands, type ChapterState, type GlossaryEntry, type Series } from '@koharu/ipc/protocol'

/** A glossary term with every field, `fields` over a new unlocked one's. */
export function term(id: string, source: string, target: string, fields: object = {}) {
  return { id, source, target, note: '', kind: 'other', locked: false, ...fields } as const
}

/** The series "Blue Lock" with two terms, one locked, and a character. */
export function blueLock(): Required<Series> {
  return {
    id: 'blue-lock-0123456789ab',
    name: 'Blue Lock',
    notes: 'A football academy.',
    glossary: [
      term('t1', '潔', 'Isagi', { kind: 'name', locked: true }),
      term('t2', 'エゴ', 'ego', { kind: 'term', note: 'Lower case.' }),
    ],
    characters: [{ id: 'c1', name: 'Isagi', original: '潔', notes: 'Polite, says "I".' }],
  }
}

/** The series "Haikyu" with nothing in it. */
export function haikyu(): Required<Series> {
  return { id: 'haikyu-ba9876543210', name: 'Haikyu', notes: '', glossary: [], characters: [] }
}

/** The desktop's series folder and the open project's side file, which the context commands read
 * and write as booharu-core does, every other command answering null; another writer, such as the
 * agent, may change either between commands. `saves` lists each series saved whole. */
export function contextDesktop({
  series = [blueLock(), haikyu()],
  linked = blueLock().id as string | null,
  suggestions = [term('s1', 'ヒナ', 'Hina', { kind: 'name' })] as GlossaryEntry[],
}: { series?: Series[]; linked?: string | null; suggestions?: GlossaryEntry[] } = {}) {
  const table = commands as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>
  for (const name of Object.keys(table)) vi.spyOn(table, name).mockResolvedValue(null)
  const files = {
    series: new Map(series.map((item) => [item.id!, structuredClone(item)])),
    chapter: { series: linked, suggestions: structuredClone(suggestions) },
  }
  const saves: Series[] = []
  const get = (id: string) => {
    const found = files.series.get(id)
    if (!found) throw `there is no series "${id}"`
    return structuredClone(found)
  }
  const state = (): ChapterState => {
    const series = files.chapter.series ? (files.series.get(files.chapter.series) ?? null) : null
    return {
      context: {
        version: 1,
        series: files.chapter.series,
        brief: null,
        suggestions: structuredClone(files.chapter.suggestions),
      },
      series: series && structuredClone(series),
      missing: files.chapter.series !== null && series === null,
      glossary: [],
      fresh: false,
      left_out: 0,
    }
  }
  const spies = {
    list: vi
      .spyOn(commands, 'booharuListSeries')
      .mockImplementation(async () =>
        [...files.series.values()]
          .map((item) => structuredClone(item))
          .sort((a, b) => a.name!.localeCompare(b.name!)),
      ),
    get: vi.spyOn(commands, 'booharuGetSeries').mockImplementation(async (id) => get(id)),
    save: vi.spyOn(commands, 'booharuSaveSeries').mockImplementation(async (saved) => {
      saves.push(structuredClone(saved))
      files.series.set(saved.id!, structuredClone(saved))
      return get(saved.id!)
    }),
    create: vi.spyOn(commands, 'booharuCreateSeries').mockImplementation(async (name) => {
      const made = { ...haikyu(), id: 'new-series-000000000000', name }
      files.series.set(made.id, made)
      return structuredClone(made)
    }),
    rename: vi.spyOn(commands, 'booharuRenameSeries').mockImplementation(async (id, name) => {
      files.series.set(id, { ...get(id), name })
      return get(id)
    }),
    delete: vi.spyOn(commands, 'booharuDeleteSeries').mockImplementation(async (id) => {
      get(id)
      files.series.delete(id)
      return null
    }),
    link: vi.spyOn(commands, 'booharuLinkSeries').mockImplementation(async (id) => {
      if (id !== null) get(id)
      files.chapter.series = id
      return state()
    }),
    chapter: vi.spyOn(commands, 'booharuGetChapterContext').mockImplementation(async () => state()),
    accept: vi.spyOn(commands, 'booharuAcceptSuggestion').mockImplementation(async (entry) => {
      const id = files.chapter.series
      if (id === null) throw 'the chapter is linked to no series; pick one first'
      const held = get(id)
      files.series.set(id, { ...held, glossary: [...held.glossary!, entry] })
      files.chapter.suggestions = files.chapter.suggestions.filter((item) => item.id !== entry.id)
      return state()
    }),
    dismiss: vi.spyOn(commands, 'booharuDismissSuggestion').mockImplementation(async (id) => {
      files.chapter.suggestions = files.chapter.suggestions.filter((item) => item.id !== id)
      return state()
    }),
  }
  return { files, saves, spies }
}

/** The project "Ch.1" open, so the chapter it is reads. */
export function openChapter(): void {
  const project: ProjectView = {
    name: 'Ch.1',
    page: null,
    pages: [],
    history: { steps: [], current: 0 },
    revisions: {},
  }
  queryClient.setQueryData(projectKey, project)
}
