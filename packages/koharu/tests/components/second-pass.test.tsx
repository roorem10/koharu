import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { ContextPreferences } from '@/components/context/ContextPreferences'
import { SecondPassButton } from '@/components/context/SecondPassButton'
import { SecondPassDetails } from '@/components/context/SecondPassDetails'
import { ActivityCenter } from '@/components/editor/ActivityCenter'
import { pageKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import translations from '@/public/locales/en-US/translation.json'
import { commands, type Job, type SecondPassResult } from '@koharu/bridge/protocol'

const project = {
  name: 'chapter',
  revision: 1,
  active_page: null,
  can_undo: true,
  can_redo: false,
}
const result: SecondPassResult = {
  previous: 'An earlier wording.',
  best: 'The best wording.',
  alternatives: ['First alternative.', 'Second alternative.'],
  reason: 'The pun uses the character’s name.',
}

function setup() {
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pageKey, null)
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  vi.spyOn(commands, 'getPage').mockResolvedValue(null)
  vi.mocked(commands.getChapterContext).mockResolvedValue({
    context: { version: 1, series: null, brief: null, suggestions: [], lines: {} },
    series: null,
    supports_tasks: false,
    supports_second_pass: true,
  })
}

function mount(children: React.ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>)
}

describe('second pass', () => {
  it('uses the selected scope and the normal job stop path with an independent hard-line model', async () => {
    setup()
    const start = vi.spyOn(commands, 'startSecondPass').mockResolvedValue('second-pass')
    const stop = vi.spyOn(commands, 'stopJob').mockResolvedValue(null)
    mount(
      <>
        <SecondPassButton scope={{ scope: 'boxes', layers: ['line'] }} />
        <SecondPassButton scope={{ scope: 'puns' }} />
        <SecondPassButton scope={{ scope: 'hard' }} />
        <ActivityCenter />
      </>,
    )
    const box = await screen.findByRole('button', { name: translations.secondPass.title })
    await waitFor(() => expect(box).toBeEnabled())
    fireEvent.click(box)
    await waitFor(() => expect(start).toHaveBeenCalledWith({ scope: 'boxes', layers: ['line'] }))
    const puns = screen.getByRole('button', { name: translations.secondPass.puns })
    await waitFor(() => expect(puns).toBeEnabled())
    fireEvent.click(puns)
    await waitFor(() => expect(start).toHaveBeenCalledWith({ scope: 'puns' }))
    const hard = screen.getByRole('button', { name: translations.secondPass.hard })
    await waitFor(() => expect(hard).toBeEnabled())
    fireEvent.click(hard)
    await waitFor(() => expect(start).toHaveBeenCalledWith({ scope: 'hard' }))
    const job: Job = {
      id: 'second-pass',
      state: 'running',
      completed: 1,
      total: 3,
      page: null,
      stage: 'translation',
      model: 'Strong model',
      error: null,
      retry: null,
      failures: [],
      project: 'chapter',
      operation: null,
      finish_sound: false,
    }
    act(() => useKoharuStore.setState({ jobs: { [job.id]: job } }))
    expect(box).toBeDisabled()
    fireEvent.click(await screen.findByRole('button', { name: translations.activity.stop }))
    await waitFor(() => expect(stop).toHaveBeenCalledWith(job.id))
  })

  it('restores earlier text and accepts alternatives through undoable translation edits', async () => {
    setup()
    vi.mocked(commands.getSecondPass).mockResolvedValue(result)
    const edit = vi.spyOn(commands, 'setTranslation').mockResolvedValue(null)
    mount(<SecondPassDetails layer='line' translation={result.best} />)
    fireEvent.click(await screen.findByRole('button', { name: translations.secondPass.restore }))
    await waitFor(() => expect(edit).toHaveBeenCalledWith('line', result.previous))
    const choices = await screen.findAllByRole('button', {
      name: translations.chapterContext.useThis,
    })
    await waitFor(() => expect(choices[1]).toBeEnabled())
    fireEvent.click(choices[1])
    await waitFor(() => expect(edit).toHaveBeenCalledWith('line', result.alternatives[1]))
  })

  it('does not show another box’s pending result and refreshes after undo changes the revision', async () => {
    setup()
    let resolve!: (value: SecondPassResult | null) => void
    vi.mocked(commands.getSecondPass).mockImplementation((layer) =>
      layer === 'first'
        ? new Promise((done) => {
            resolve = done
          })
        : Promise.resolve(null),
    )
    const view = mount(<SecondPassDetails layer='first' translation={result.best} />)
    await waitFor(() => expect(commands.getSecondPass).toHaveBeenCalledWith('first'))
    view.rerender(
      <QueryClientProvider client={queryClient}>
        <SecondPassDetails layer='second' translation={result.best} />
      </QueryClientProvider>,
    )
    await act(async () => resolve(result))
    expect(screen.queryByText(result.reason)).not.toBeInTheDocument()
    vi.mocked(commands.getSecondPass).mockResolvedValue(result)
    act(() => queryClient.setQueryData(projectKey, { ...project, revision: 2 }))
    expect(await screen.findByText(result.reason)).toBeInTheDocument()
    vi.mocked(commands.getSecondPass).mockResolvedValue(null)
    act(() => queryClient.setQueryData(projectKey, { ...project, revision: 3 }))
    await waitFor(() => expect(screen.queryByText(result.reason)).not.toBeInTheDocument())
  })

  it('saves the hard-line override separately and can return to the normal model', async () => {
    setup()
    const selection = {
      provider: 'openai',
      model: 'strong',
      quantization: null,
      vision: true,
      reasoning: false,
    } as const
    vi.mocked(commands.getReadingModels).mockResolvedValue([
      { ...selection, name: 'Stronger model', quantizations: [] },
    ])
    vi.mocked(commands.getContextSettings).mockResolvedValue({
      read_when: 'missing',
      send_images: true,
      ask_before_reading: true,
      read_model: null,
      hard_line_model: null,
      pages_per_request: 20,
      previous_pages: 1,
      agent_messages: 20,
    })
    mount(<ContextPreferences />)
    fireEvent.click(await screen.findByRole('button', { name: translations.secondPass.model }))
    fireEvent.click(await screen.findByRole('button', { name: /Stronger model/ }))
    await waitFor(() =>
      expect(commands.saveContextSettings).toHaveBeenCalledWith(
        expect.objectContaining({ hard_line_model: selection, read_model: null }),
      ),
    )
    const reset = (
      await screen.findAllByRole('button', { name: translations.chapterContext.sameModel })
    ).find((button) => !button.hasAttribute('aria-haspopup'))!
    await waitFor(() => expect(reset).toBeEnabled())
    fireEvent.click(reset)
    await waitFor(() =>
      expect(commands.saveContextSettings).toHaveBeenLastCalledWith(
        expect.objectContaining({ hard_line_model: null, read_model: null }),
      ),
    )
  })
})
