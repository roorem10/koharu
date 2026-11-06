import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { offerTranslationTerm, TermLearningOffer } from '@/components/context/TermLearningOffer'
import { TranslationTextarea } from '@/components/context/TranslationTextarea'
import { pageKey, projectKey, queryClient, usePage } from '@/lib/queries'
import translation from '@/public/locales/en-US/translation.json'
import {
  commands,
  type ChapterState,
  type Layer,
  type Page,
  type ProjectInfo,
} from '@koharu/bridge/protocol'

let fixture = 0
function setup(text = 'Hinna, come here!') {
  const id = `learn-${++fixture}`
  const project: ProjectInfo = {
    name: id,
    revision: 2,
    active_page: 'page',
    can_undo: true,
    can_redo: false,
  }
  const layer: Extract<Layer, { type: 'text' }> = {
    id,
    type: 'text',
    parent: null,
    geometry: null,
    angle_degrees: null,
    visibility: { visible: true, opacity: 1 },
    typography: null,
    layout: 'paragraph',
    automatic_region: null,
    content: {
      id: `content-${id}`,
      source: { text: 'ヒナ、来て！', language: 'ja' },
      translation: { text, language: 'en' },
      role: null,
      source_region: null,
    },
  }
  let page: Page = {
    id: 'page',
    label: 'Page',
    size: { width: 100, height: 100 },
    layers: [layer],
    regions: [],
  }
  const chapter: ChapterState = {
    series: {
      id: 'series',
      name: 'Series',
      notes: '',
      characters: [],
      glossary: [
        { id: 'hina', source: 'ヒナ', target: 'Hina', locked: true, note: '', kind: 'name' },
      ],
    },
    context: { version: 1, series: 'series', suggestions: [], lines: {}, brief: null },
    supports_tasks: true,
    term_conflicts: [],
  }
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pageKey, page)
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  vi.spyOn(commands, 'getPage').mockImplementation(async () => page)
  vi.mocked(commands.getChapterContext).mockResolvedValue(chapter)
  const claim = vi.spyOn(commands, 'claimTermOffer').mockResolvedValue(true)
  const update = vi.spyOn(commands, 'updateSeriesTerm').mockResolvedValue(null)
  const usage = vi
    .spyOn(commands, 'glossaryUsage')
    .mockResolvedValue({ layers: [id, 'automatic'], skipped: 1 })
  const process = vi.spyOn(commands, 'process').mockResolvedValue('run')
  const setText = (value: string) => {
    page = {
      ...page,
      layers: [
        { ...layer, content: { ...layer.content, translation: { text: value, language: 'en' } } },
      ],
    }
    queryClient.setQueryData(pageKey, page)
  }
  const offer = () =>
    offerTranslationTerm(id, 'ヒナ、来て！', 'Hina, come here!', 'Hinna, come here!', project.name)
  return { id, project, layer, chapter, claim, update, usage, process, setText, offer }
}

function show(layer: string) {
  return render(
    <QueryClientProvider client={queryClient}>
      <TermLearningOffer layer={layer} />
    </QueryClientProvider>,
  )
}

describe('hand-edit term offer', () => {
  it('keeps the project identity captured before the hand-edit save', async () => {
    const state = setup()
    queryClient.setQueryData(projectKey, { ...state.project, name: 'different' })
    await state.offer()
    expect(state.claim).not.toHaveBeenCalled()
  })

  it('waits for explicit acceptance, saves with the observed spelling, and reuses the term translation path', async () => {
    const state = setup()
    show(state.id)
    await act(state.offer)
    expect(state.update).not.toHaveBeenCalled()
    fireEvent.click(
      screen.getByRole('button', { name: translation.chapterContext.useEverywhereAndSave }),
    )
    await waitFor(() =>
      expect(state.process).toHaveBeenCalledWith(
        { scope: 'entities', value: [state.id, 'automatic'] },
        { operation: 'only', stage: 'translation' },
      ),
    )
    expect(state.update).toHaveBeenCalledWith('series', 'ヒナ', 'Hina', 'Hinna', state.project.name)
  })

  it('does not repeat an offer already recorded in native context', async () => {
    const state = setup()
    state.claim.mockResolvedValue(false)
    show(state.id)
    await act(state.offer)
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
    expect(state.update).not.toHaveBeenCalled()
  })

  it('does not claim an offer after switching projects, even when copied layer IDs match', async () => {
    const state = setup()
    let finish!: (chapter: ChapterState) => void
    vi.mocked(commands.getChapterContext).mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    const task = state.offer()
    queryClient.setQueryData(projectKey, { ...state.project, name: 'different' })
    finish(state.chapter)
    await task
    expect(state.claim).not.toHaveBeenCalled()
  })

  it('discards a delayed claim when the project switches', async () => {
    const state = setup()
    let finish!: (claimed: boolean) => void
    state.claim.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    show(state.id)
    const task = state.offer()
    await waitFor(() => expect(state.claim).toHaveBeenCalled())
    await act(async () => {
      queryClient.setQueryData(projectKey, { ...state.project, name: 'different' })
      finish(true)
      await task
      queryClient.setQueryData(projectKey, state.project)
    })
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
  })

  it('does not start translation in a project opened while the series save was pending', async () => {
    const state = setup()
    let finish!: () => void
    state.update.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = () => resolve(null)
        }),
    )
    show(state.id)
    await act(state.offer)
    fireEvent.click(
      screen.getByRole('button', { name: translation.chapterContext.useEverywhereAndSave }),
    )
    await act(async () => {
      vi.mocked(commands.getProject).mockResolvedValue({ ...state.project, name: 'different' })
      queryClient.setQueryData(projectKey, { ...state.project, name: 'different' })
      finish()
    })
    expect(state.usage).not.toHaveBeenCalled()
    expect(state.process).not.toHaveBeenCalled()
  })

  it('learns the complete focused edit rather than an intermediate autosaved spelling', async () => {
    const state = setup('Hina, come here!')
    const set = vi.spyOn(commands, 'setTranslation').mockImplementation(async (_layer, text) => {
      state.setText(text ?? '')
      return null
    })
    function Editor() {
      const layer = usePage().data?.layers[0] as typeof state.layer
      return <TranslationTextarea layer={layer} aria-label='translation' />
    }
    render(
      <QueryClientProvider client={queryClient}>
        <Editor />
      </QueryClientProvider>,
    )
    const input = screen.getByRole('textbox')
    fireEvent.focus(input)
    fireEvent.change(input, { target: { value: 'Hinn, come here!' } })
    await waitFor(() => expect(set).toHaveBeenCalledWith(state.id, 'Hinn, come here!'))
    expect(state.claim).not.toHaveBeenCalled()
    fireEvent.change(input, { target: { value: 'Hinna, come here!' } })
    fireEvent.blur(input)
    await waitFor(() =>
      expect(state.claim).toHaveBeenCalledWith(
        'series',
        'ヒナ',
        'Hina',
        'Hinna',
        state.project.name,
      ),
    )
    expect(state.claim).toHaveBeenCalledTimes(1)
  })
})
