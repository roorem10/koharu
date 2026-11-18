'use client'

import { useQuery } from '@tanstack/react-query'
import { useEffect } from 'react'

import { call, refreshPreferences } from '@/lib/backend'
import { queueBooharuWrite } from '@/lib/booharu/preferences'
import { projectKey, queryClient, refresh, useProject } from '@/lib/queries'
import {
  commands,
  events,
  type ChapterState,
  type Character,
  type FirstStartNote,
  type FixedTerm,
  type GlossaryEntry,
  type InstructionImport,
  type InstructionPreview,
  type Series,
  type SeriesImport,
  type TermKind,
} from '@koharu/ipc/protocol'

/** `T` with every field there, as the desktop sends it: the bindings mark the fields serde
 * defaults optional, as a series file may leave them out, but the desktop always sends them. */
export type Filled<T> = T extends readonly (infer Item)[]
  ? Filled<Item>[]
  : T extends object
    ? { [Key in keyof T]-?: Filled<T[Key]> }
    : T

/** A series: its glossary, characters and notes. */
export type FullSeries = Filled<Series>
/** A glossary term. */
export type FullEntry = Filled<GlossaryEntry>
/** A character of a series. */
export type FullCharacter = Filled<Character>
/** The open project's chapter: its side file, the series it links, and the glossary a
 * translation run on it would send. */
export type FullChapter = Filled<ChapterState>

/** The kinds of glossary term, in the order the term form lists them. */
export const termKinds: readonly TermKind[] = ['name', 'place', 'term', 'honorific', 'other']

/** The string key of `kind`'s name. */
export function kindName(kind: TermKind) {
  return `booharu.context.kinds.${kind}` as const
}

function fillEntry(entry: GlossaryEntry): FullEntry {
  return {
    id: entry.id ?? '',
    source: entry.source ?? '',
    target: entry.target ?? '',
    note: entry.note ?? '',
    kind: entry.kind ?? 'other',
    locked: entry.locked ?? false,
  }
}

function fillCharacter(character: Character): FullCharacter {
  return {
    id: character.id ?? '',
    name: character.name ?? '',
    original: character.original ?? '',
    notes: character.notes ?? '',
  }
}

/** `series` with every field, a missing one taking booharu-core's default. */
export function fillSeries(series: Series): FullSeries {
  return {
    id: series.id ?? '',
    name: series.name ?? '',
    notes: series.notes ?? '',
    glossary: (series.glossary ?? []).map(fillEntry),
    characters: (series.characters ?? []).map(fillCharacter),
  }
}

function fillChapter(chapter: ChapterState): FullChapter {
  const { context } = chapter
  const brief = context.brief ?? null
  return {
    ...chapter,
    context: {
      version: context.version ?? 1,
      series: context.series ?? null,
      brief: brief && {
        summary: brief.summary ?? '',
        characters: (brief.characters ?? []).map((character) => ({
          name: character.name ?? '',
          original: character.original ?? '',
          pages: character.pages ?? [],
        })),
        edited: brief.edited ?? false,
        read_at: brief.read_at ?? null,
      },
      suggestions: (context.suggestions ?? []).map(fillEntry),
    },
    series: chapter.series && fillSeries(chapter.series),
    glossary: chapter.glossary.map(fillEntry),
  }
}

/** `text` as terms are compared, as booharu-core's matching does: without whitespace, in lower
 * case. */
function termKey(text: string): string {
  return text.replace(/\s/gu, '').toLowerCase()
}

/** Whether `a` and `b`, two terms' sources, name the same term: ignoring case and whitespace, as
 * booharu-core compares them. */
export function sameSource(a: string, b: string): boolean {
  return termKey(a) === termKey(b)
}

/** Whether `entry` shows for the search `search`: its source, translation or note holds it,
 * ignoring case. */
export function matchesSearch(entry: FullEntry, search: string): boolean {
  const wanted = search.trim().toLowerCase()
  return `${entry.source} ${entry.target} ${entry.note}`.toLowerCase().includes(wanted)
}

/** A new term of `source` and `target`, unlocked, with a new id, of the kind Term, as Booharu
 * 0.0.4's glossary editor started one. */
export function newTerm(source = '', target = ''): FullEntry {
  return { id: crypto.randomUUID(), source, target, note: '', kind: 'term', locked: false }
}

/** A new character with nothing written yet. */
export function newCharacter(): FullCharacter {
  return { id: crypto.randomUUID(), name: '', original: '', notes: '' }
}

/** The term of `glossary`, other than `entry` itself, that saving `entry` would replace: the
 * first with its source. */
export function replacedTerm(glossary: readonly FullEntry[], entry: FullEntry): FullEntry | null {
  return (
    glossary.find((held) => held.id !== entry.id && sameSource(held.source, entry.source)) ?? null
  )
}

/** `series` with `entry` saved in its glossary, its source, translation and note trimmed: in
 * place of the term of its id, or else of the first term with its source, keeping that term's
 * place and id, as booharu-core puts a term; else after the others. Any other term with its source
 * goes, so the series ends with one term for it. */
export function putTerm(series: FullSeries, entry: FullEntry): FullSeries {
  const term: FullEntry = {
    ...entry,
    source: entry.source.trim(),
    target: entry.target.trim(),
    note: entry.note.trim(),
  }
  let at = series.glossary.findIndex((held) => held.id === term.id)
  if (at < 0) at = series.glossary.findIndex((held) => sameSource(held.source, term.source))
  if (at < 0) return { ...series, glossary: [...series.glossary, term] }
  const placed = { ...term, id: series.glossary[at]?.id || term.id }
  return {
    ...series,
    glossary: series.glossary.flatMap((held, index) => {
      if (index === at) return [placed]
      return sameSource(held.source, term.source) ? [] : [held]
    }),
  }
}

/** `series` without its term `id`. */
export function removeTerm(series: FullSeries, id: string): FullSeries {
  return { ...series, glossary: series.glossary.filter((term) => term.id !== id) }
}

/** `series` with its character `id` as `change` leaves it. */
export function changeCharacter(
  series: FullSeries,
  id: string,
  change: Partial<FullCharacter>,
): FullSeries {
  return {
    ...series,
    characters: series.characters.map((character) =>
      character.id === id ? { ...character, ...change, id } : character,
    ),
  }
}

/** The key of every series read: the list's, and each series' under it, so a change to any series
 * reads them all again. */
export const seriesKey = ['booharu', 'series'] as const

/** The key of the open project's chapter, under the project's, so the change, undo or redo of any
 * page, which reads the project again, reads the chapter again too: the glossary a translation
 * would send follows the text on its pages. */
export const chapterKey = [...projectKey, 'booharu', 'chapter'] as const

const fillAll = (list: Series[]) => list.map(fillSeries)

/** Every series, by name, read while a view showing them is mounted and again after every change
 * to one. A failed read shows its error, as every failed command's does. */
export function useSeriesList() {
  return useQuery({
    queryKey: seriesKey,
    queryFn: () => call(commands.booharuListSeries),
    select: fillAll,
  })
}

/** The series `id`, none while `id` is, read again after every change to a series. A failed read,
 * as of a file that doesn't parse, shows no toast: the view says why. */
export function useSeries(id: string | null) {
  return useQuery({
    queryKey: [...seriesKey, id ?? ''],
    queryFn: () => commands.booharuGetSeries(id ?? ''),
    enabled: id !== null,
    select: fillSeries,
  })
}

/** The open project's chapter, read while a view showing it is mounted, again after every change
 * to the project, a series or the chapter, and for each project anew, so one project's chapter
 * never shows for another. A failed read, as of a side file that doesn't parse, shows no toast, as
 * every change reads it again: the view shows the error, which names the file. */
export function useChapterContext() {
  const project = useProject().data?.name ?? null
  return useQuery({
    queryKey: [...chapterKey, project ?? ''],
    queryFn: () => commands.booharuGetChapterContext(),
    enabled: project !== null,
    select: fillChapter,
  })
}

/** The message of a failed read, which the desktop rejects with as a string. */
export function failure(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/** Reads every series and the open project's chapter again, in the views showing them. */
export function refreshContext(): Promise<void> {
  return refresh(seriesKey, chapterKey)
}

/** Reads the series and the chapter again whenever the desktop tells `booharu_context_changed`,
 * after a write to a series file or the open project's side file, by the interface or by the
 * agent, until the interface unmounts. */
export function useBooharuContextChanges(): void {
  useEffect(() => {
    let active = true
    const listening = events.booharuContextChanged(() => {
      if (active) void refreshContext().catch(() => undefined)
    })
    return () => {
      active = false
      void listening.then((stop) => stop()).catch(() => undefined)
    }
  }, [])
}

/** The last write to the series or the chapter, which the next one waits for. */
let writing: Promise<unknown> = Promise.resolve()

/** Runs `write`, a command that writes a series file or the open project's side file, once the
 * writes queued before it end, so a save, which writes a series as it read it just before, never
 * puts back what another of the interface's writes changed in between; then reads the series and
 * the chapter again, whether it worked or not, before it settles. A failure shows its error, as
 * every failed command's does, and rejects. */
function queueWrite<T>(write: () => Promise<T>): Promise<T> {
  const done = writing
    .catch(() => undefined)
    .then(write)
    .finally(() => refreshContext().catch(() => undefined))
  writing = done
  return done
}

/** Links the open project's chapter to the series `series`, or to none. */
export function linkSeries(series: string | null): Promise<ChapterState> {
  return queueWrite(() => call(commands.booharuLinkSeries, series))
}

/** Makes a series named `name`, trimmed, with nothing in it. */
export async function createSeries(name: string): Promise<FullSeries> {
  return fillSeries(await queueWrite(() => call(commands.booharuCreateSeries, name.trim())))
}

/** Names the series `id` `name`, trimmed. */
export function renameSeries(id: string, name: string): Promise<Series> {
  return queueWrite(() => call(commands.booharuRenameSeries, id, name.trim()))
}

/** Deletes the series `id`; the chapters linking it then link a series that's gone. */
export function deleteSeries(id: string): Promise<null> {
  return queueWrite(() => call(commands.booharuDeleteSeries, id))
}

/** Saves `change`, made to the series `id` as its file holds it just before the save, so a term
 * the agent added meanwhile stays. */
export function changeSeries(
  id: string,
  change: (series: FullSeries) => FullSeries,
): Promise<FullSeries> {
  return queueWrite(async () => {
    const current = fillSeries(await call(commands.booharuGetSeries, id))
    return fillSeries(await call(commands.booharuSaveSeries, change(current)))
  })
}

/** Accepts the open chapter's suggestion `entry`, as it holds it, into the series the chapter
 * links, in place of the series' term with its source. */
export function acceptSuggestion(entry: FullEntry): Promise<ChapterState> {
  return queueWrite(() => call(commands.booharuAcceptSuggestion, entry))
}

/** Dismisses the open chapter's suggestion `id`. */
export function dismissSuggestion(id: string): Promise<ChapterState> {
  return queueWrite(() => call(commands.booharuDismissSuggestion, id))
}

/** Runs `command` once the writes queued before it end, so it reads the series and the chapter as
 * they left them, without holding back the writes queued after it. A failure shows its error, as
 * every failed command's does, and rejects. */
async function afterWrites<T>(command: () => Promise<T>): Promise<T> {
  await writing.catch(() => undefined)
  return command()
}

/** `text`, a line's or a selection's, as a term's source or translation: on one line, its runs of
 * whitespace one space each, trimmed. Terms are matched ignoring whitespace, so this changes no
 * match. */
export function termText(text: string): string {
  return text.replace(/\s+/gu, ' ').trim()
}

/** The key of where terms are used in the open project, under the project's, so the change, undo
 * or redo of any page reads it again: a run that translates a line, or a fix by hand, changes what
 * re-translating the lines using a term would do. */
export const usageKey = [...projectKey, 'booharu', 'usage'] as const

/** Where the term `source` is used in the open project: the lines whose source it occurs in, and
 * which of them are fixed by hand or locked, read for each project anew, and not while none is
 * open. A failed read shows no toast: the view says why. */
export function useGlossaryUsage(source: string) {
  const project = useProject().data?.name ?? null
  return useQuery({
    queryKey: [...usageKey, project ?? '', source],
    queryFn: () => commands.booharuGlossaryUsage(source),
    enabled: project !== null && source.trim() !== '',
  })
}

/** Re-translates the lines using the term `source`, Fix it everywhere: one ordinary Translation run
 * on every line of the open project using it that's neither locked nor fixed by hand, untranslated
 * ones included, queued once the writes before it end, so the run follows the glossary as just
 * saved. Answers the run, none when no line was left to translate, and how many lines it translates
 * and left alone. */
export function fixTermEverywhere(source: string): Promise<FixedTerm> {
  return afterWrites(() => call(commands.booharuFixTermEverywhere, source))
}

/** Exports the series `id` to a file the user picks, once the writes before it end, so the file
 * holds every change made. Answers whether it was written, false when the user cancelled. */
export function exportSeries(id: string): Promise<boolean> {
  return afterWrites(() => call(commands.booharuExportSeries, id))
}

/** Asks the user for a series file, Booharu 0.0.4's or 0.0.5's, and answers its series with whether
 * one of its id is saved already; none when the user cancelled. */
export function previewSeriesImport(): Promise<SeriesImport | null> {
  return afterWrites(() => call(commands.booharuPreviewSeriesImport))
}

/** Saves `series`, from a file being imported: in place of the series of its id with `replace`,
 * else beside it, with a new id when that one is taken. Answers it as saved. */
export async function importSeries(series: Series, replace: boolean): Promise<FullSeries> {
  return fillSeries(await queueWrite(() => call(commands.booharuImportSeries, series, replace)))
}

/** The Instructions as saved, with the terms their lines write, for Import from instructions. */
export function previewInstructionGlossary(): Promise<InstructionPreview> {
  return call(commands.booharuPreviewInstructionGlossary)
}

/** Imports the terms on the lines `lines` of `instructions`, the Instructions as previewed, into
 * the series `series` as locked terms, skipping sources it has; with `remove`, the desktop saves the
 * Instructions without those lines, refusing when they changed since the preview. Koharu tells no
 * change of its preferences, so they're read again then, whether it worked or not: Settings shows
 * the Instructions as saved, and never saves the old text back. */
export async function importInstructionGlossary(
  series: string,
  lines: number[],
  remove: boolean,
  instructions: string,
): Promise<InstructionImport> {
  try {
    return await queueWrite(() =>
      call(commands.booharuImportInstructionGlossary, series, lines, remove, instructions),
    )
  } finally {
    if (remove) await refreshPreferences().catch(() => undefined)
  }
}

/** The key of the note on a first start. */
export const firstStartNoteKey = ['booharu', 'first-start-note'] as const

/** The note on a first start: whether to show it, and where Booharu 0.0.4's series are, read once
 * a session. It names the app's query client itself, as Koharu's home page needs no query provider
 * around it, and its own tests render it without one; a failed read shows no note. */
export function useFirstStartNote() {
  return useQuery(
    {
      queryKey: firstStartNoteKey,
      queryFn: () => commands.booharuGetFirstStartNote(),
    },
    queryClient,
  )
}

/** Dismisses the note on a first start for good; it goes once the desktop has saved that. The
 * desktop saves `[booharu-start]` alone, in turn with the settings pages' section saves, so none of
 * those, which save every section as they read them, puts the note back. */
export async function dismissFirstStartNote(): Promise<void> {
  await queueBooharuWrite(() => call(commands.booharuDismissFirstStartNote))
  queryClient.setQueryData<FirstStartNote>(firstStartNoteKey, (note) =>
    note ? { ...note, show: false } : note,
  )
}
