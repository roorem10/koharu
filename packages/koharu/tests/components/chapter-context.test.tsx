import { QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { ContextPanel } from '@/components/context/ContextPanel'
import { GlossaryTextMenu } from '@/components/context/GlossaryTextMenu'
import { LineContextCard } from '@/components/editor/LineContextCard'
import { pagesKey, projectKey, queryClient } from '@/lib/queries'
import { commands, type ChapterState, type Series } from '@koharu/bridge/protocol'

const series: Series = {
  id: 'test-123',
  name: 'Test',
  notes: '',
  characters: [],
  glossary: [{ id: 'hina', source: 'ヒナ', target: 'Hina', kind: 'name', locked: true, note: '' }],
}
const chapter: ChapterState = {
  context: {
    version: 1,
    series: series.id,
    brief: { summary: 'A quiet reunion.', characters: [], edited: false, read_at: null },
    suggestions: [],
    lines: {},
  },
  series,
  supports_tasks: true,
}

function setup() {
  const project = {
    name: 'chapter',
    revision: 1,
    active_page: null,
    can_undo: false,
    can_redo: false,
  }
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pagesKey, [])
  vi.mocked(commands.getChapterContext).mockResolvedValue(chapter)
  vi.mocked(commands.listSeries).mockResolvedValue([series])
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  vi.spyOn(commands, 'getPage').mockResolvedValue(null)
}

describe('chapter context', () => {
  it('renders the brief and glossary in the Context panel', async () => {
    setup()
    render(
      <QueryClientProvider client={queryClient}>
        <ContextPanel />
      </QueryClientProvider>,
    )
    fireEvent.click(await screen.findByRole('button', { name: 'View' }))
    expect(await screen.findByRole('textbox', { name: 'Chapter brief' })).toHaveValue(
      'A quiet reunion.',
    )
    expect(screen.getByText(/ヒナ → Hina/)).toBeInTheDocument()
  })

  it('adds selected source text to the glossary through the editable-field menu', async () => {
    setup()
    vi.spyOn(commands, 'getSeries').mockResolvedValue(series)
    const save = vi.spyOn(commands, 'saveSeries').mockImplementation(async (value) => value)
    vi.spyOn(commands, 'glossaryUsage').mockResolvedValue({ layers: [], skipped: 0 })
    render(
      <QueryClientProvider client={queryClient}>
        <GlossaryTextMenu field='source' source='魔王です' translation='The Demon King'>
          <textarea aria-label='OCR' defaultValue='魔王です' />
        </GlossaryTextMenu>
      </QueryClientProvider>,
    )
    const textarea = screen.getByLabelText('OCR') as HTMLTextAreaElement
    textarea.setSelectionRange(0, 2)
    fireEvent.contextMenu(textarea)
    fireEvent.click(await screen.findByText('Add to glossary'))
    expect(await screen.findByLabelText('Source')).toHaveValue('魔王')
    fireEvent.click(screen.getByRole('checkbox', { name: 'Lock this translation' }))
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(save.mock.calls[0][0].glossary).toContainEqual(
      expect.objectContaining({ source: '魔王', target: 'The Demon King', locked: true }),
    )
  })

  it('uses an explanation alternative through the existing undoable translation command', async () => {
    setup()
    vi.spyOn(commands, 'explainLine').mockResolvedValue({
      explanation: 'The line is a pun.',
      alternatives: ['A first wording.', 'A second wording.'],
    })
    const setTranslation = vi.spyOn(commands, 'setTranslation').mockResolvedValue(null)
    render(
      <QueryClientProvider client={queryClient}>
        <LineContextCard layer='line' />
      </QueryClientProvider>,
    )
    fireEvent.click(await screen.findByRole('button', { name: 'Explain this line' }))
    const buttons = await screen.findAllByRole('button', { name: 'Use this' })
    fireEvent.click(buttons[0])
    await waitFor(() => expect(setTranslation).toHaveBeenCalledWith('line', 'A first wording.'))
  })
})
