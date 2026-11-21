import { QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { createElement, type ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

import {
  changeSeries,
  fillSeries,
  matchesSearch,
  putTerm,
  sameSource,
  termText,
  useBooharuContextChanges,
  useChapterContext,
  useSeriesList,
} from '@/lib/booharu/context'
import { preferencesKey, queryClient, usePreferences } from '@/lib/queries'
import { commands, events, type Preferences } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { blueLock, contextDesktop, openChapter, term } from './context'

const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(QueryClientProvider, { client: queryClient }, children)

describe('Booharu glossary terms', () => {
  it('compares sources ignoring case and whitespace, as booharu-core does', () => {
    expect(sameSource(' Hina  Chan', 'hinachan')).toBe(true)
    expect(sameSource('潔　世一', '潔世一')).toBe(true)
    expect(sameSource('Hina', 'Hinata')).toBe(false)
  })

  it('takes a line’s text onto one line, joining Chinese and Japanese across a line break', () => {
    // A vertical bubble's column break, or a selection across one, joins its words.
    expect(termText('潔の\n武器')).toBe('潔の武器')
    expect(termText(' 魔\r\n 王 ')).toBe('魔王')
    expect(termText('何だ！？\nお前')).toBe('何だ！？お前')
    expect(termText('ヒナ\u3000\nさん')).toBe('ヒナさん')
    // Elsewhere a line break is a space, as is every other run of whitespace.
    expect(termText('Blue\nLock')).toBe('Blue Lock')
    expect(termText('Isagi\n潔')).toBe('Isagi 潔')
    expect(termText('파란\n자물쇠')).toBe('파란 자물쇠')
    expect(termText('潔  世一')).toBe('潔 世一')
  })

  it('finds a term by its source, translation or note, ignoring case', () => {
    const entry = term('t', 'エゴ', 'Ego', { note: 'Lower case' })
    expect(['えご', 'ego', 'LOWER', ''].map((search) => matchesSearch(entry, search))).toEqual([
      false,
      true,
      true,
      true,
    ])
  })

  it('saves a term in place of its own, or of the first with its source, dropping the rest', () => {
    const series = fillSeries({
      ...blueLock(),
      glossary: [
        term('a', 'ヒナ', 'Hina'),
        term('b', 'エゴ', 'ego'),
        term('c', 'ひな', 'Hinako'),
        term('d', ' ヒナ ', 'Hina-chan'),
      ],
    })
    const sources = (glossary: { id: string; source: string; target: string }[]) =>
      glossary.map(({ id, source, target }) => `${id}:${source}=${target}`)

    // Edited by id: its source changes, and another term with that source goes.
    expect(sources(putTerm(series, { ...series.glossary[2]!, source: 'エゴ ' }).glossary)).toEqual([
      'a:ヒナ=Hina',
      'c:エゴ=Hinako',
      'd: ヒナ =Hina-chan',
    ])
    // A new term takes the place and id of the first with its source; later ones go.
    expect(sources(putTerm(series, term('new', 'ヒナ', ' Hinata ')).glossary)).toEqual([
      'a:ヒナ=Hinata',
      'b:エゴ=ego',
      'c:ひな=Hinako',
    ])
    // One with a source no term has goes after the others.
    expect(sources(putTerm(series, term('new', '凛', 'Rin')).glossary).at(-1)).toBe('new:凛=Rin')
  })
})

describe('Booharu context data', () => {
  it('saves a change over the series as its file holds it just before', async () => {
    const { files, saves } = contextDesktop()
    // The agent adds a term after the editor read the series.
    const id = blueLock().id
    files.series.get(id)!.glossary!.push(term('agent', '凛', 'Rin'))

    await changeSeries(id, (series) => ({ ...series, notes: 'Football.' }))

    expect(saves).toHaveLength(1)
    expect(saves[0]!.notes).toBe('Football.')
    expect(saves[0]!.glossary!.map((entry) => entry.source)).toEqual(['潔', 'エゴ', '凛'])
  })

  it('reads the series and the chapter again as the desktop tells they changed', async () => {
    const { files, spies } = contextDesktop()
    openChapter()
    let tell: () => void = () => undefined
    const stop = vi.fn()
    vi.spyOn(events, 'booharuContextChanged').mockImplementation(async (handler) => {
      tell = () => handler(null)
      return stop
    })
    vi.spyOn(events, 'booharuInstructionsChanged').mockResolvedValue(stop)
    renderHook(() => useBooharuContextChanges(), { wrapper })
    const list = renderHook(() => useSeriesList(), { wrapper })
    const chapter = renderHook(() => useChapterContext(), { wrapper })
    await waitFor(() => expect(chapter.result.current.data?.series?.name).toBe('Blue Lock'))
    expect(list.result.current.data?.map((series) => series.name)).toEqual(['Blue Lock', 'Haikyu'])

    // The agent renames the series and dismisses the suggestion.
    files.series.get(blueLock().id)!.name = 'Blue Lock: Episode Nagi'
    files.chapter.suggestions = []
    tell()

    await waitFor(() =>
      expect(chapter.result.current.data?.series?.name).toBe('Blue Lock: Episode Nagi'),
    )
    expect(chapter.result.current.data?.context.suggestions).toEqual([])
    await waitFor(() => expect(list.result.current.data?.[0]?.name).toBe('Blue Lock: Episode Nagi'))
    expect(spies.chapter).toHaveBeenCalledTimes(2)
  })

  it('reads Koharu’s settings again as the desktop tells the agent saved the Instructions', async () => {
    contextDesktop()
    // The Instructions as the interface read them, then as the agent's import left them.
    const settings = (instructions: string): Preferences => ({
      pipeline: { translator: { instructions } },
      renderer: { font_families: [] },
      agent: {},
      providers: {},
    })
    queryClient.setQueryData(preferencesKey, settings('Keep honorifics.\n潔 = Isagi'))
    const read = vi
      .spyOn(commands, 'getPreferences')
      .mockResolvedValue(settings('Keep honorifics.'))
    // The editor reads Koharu's settings while it's open, as its model menu shows them.
    const shown = renderHook(() => usePreferences(), { wrapper })
    let tell: () => void = () => undefined
    const stop = vi.fn()
    vi.spyOn(events, 'booharuContextChanged').mockResolvedValue(stop)
    vi.spyOn(events, 'booharuInstructionsChanged').mockImplementation(async (handler) => {
      tell = () => handler(null)
      return stop
    })
    const hook = renderHook(() => useBooharuContextChanges(), { wrapper })
    await waitFor(() => expect(events.booharuInstructionsChanged).toHaveBeenCalledOnce())
    expect(read).not.toHaveBeenCalled()

    tell()

    // Every later save of the cached settings, such as the Run popover's choice of model, then
    // keeps the Instructions without the imported lines.
    await waitFor(() => expect(shown.result.current.data).toEqual(settings('Keep honorifics.')))
    expect(queryClient.getQueryData(preferencesKey)).toEqual(settings('Keep honorifics.'))
    expect(read).toHaveBeenCalledOnce()
    hook.unmount()
    await waitFor(() => expect(stop).toHaveBeenCalledTimes(2))
  })

  it('shows a side file that does not read as the read’s error, without a toast', async () => {
    const { spies } = contextDesktop()
    openChapter()
    const shown = vi.spyOn(toast, 'add')
    spies.chapter.mockRejectedValue('Ch.1.booharu.json is not a chapter file: expected value')

    const chapter = renderHook(() => useChapterContext(), { wrapper })

    await waitFor(() =>
      expect(chapter.result.current.error).toBe(
        'Ch.1.booharu.json is not a chapter file: expected value',
      ),
    )
    expect(shown).not.toHaveBeenCalled()
  })
})
