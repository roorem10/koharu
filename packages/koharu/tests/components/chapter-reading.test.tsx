import { QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { ContextPanel } from '@/components/context/ContextPanel'
import { ContextPreferences } from '@/components/context/ContextPreferences'
import { contextSettingsKey } from '@/components/context/data'
import { ReadChapterButton } from '@/components/context/ReadChapterButton'
import { projectKey, queryClient } from '@/lib/queries'
import { commands, type ReadModelInfo } from '@koharu/bridge/protocol'

function setup(info: Partial<ReadModelInfo> = {}) {
  const model: ReadModelInfo = {
    selection: { provider: 'openai', model: 'Reader', vision: true },
    vision: true,
    local: false,
    label: 'Reader',
    ...info,
  }
  vi.mocked(commands.getReadingModel).mockResolvedValue(model)
  vi.mocked(commands.getChapterContext).mockResolvedValue({
    context: { version: 1, series: 'series', brief: null, suggestions: [], lines: {} },
    series: { id: 'series', name: 'Series', notes: '', characters: [], glossary: [] },
    supports_tasks: true,
  })
  queryClient.setQueryData(projectKey, {
    name: 'chapter',
    revision: 1,
    active_page: null,
    can_undo: false,
    can_redo: false,
  })
  return model
}
function mount(element = <ReadChapterButton scope={{ scope: 'project' }} />) {
  return render(<QueryClientProvider client={queryClient}>{element}</QueryClientProvider>)
}
async function clickRead() {
  const button = await screen.findByRole('button', { name: 'Read chapter' })
  await waitFor(() => expect(button).toBeEnabled())
  fireEvent.click(button)
}

describe('chapter reading confirmation', () => {
  it('warns about API credits for a cloud model and saves Do not ask again', async () => {
    const info = setup()
    mount()
    await clickRead()
    expect(await screen.findByText(/This will use API credits/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('checkbox', { name: "Don't ask again" }))
    fireEvent.click(screen.getByRole('button', { name: 'Yes' }))
    await waitFor(() =>
      expect(commands.saveContextSettings).toHaveBeenCalledWith(
        expect.objectContaining({ ask_before_reading: false }),
      ),
    )
    expect(commands.readChapter).toHaveBeenCalledWith({ scope: 'project' }, info.selection)
  })
  it('confirms that a local vision model reads on this computer', async () => {
    setup({ local: true })
    mount()
    await clickRead()
    expect(await screen.findByText(/on this computer/)).toBeInTheDocument()
    expect(screen.queryByText(/API credits/)).not.toBeInTheDocument()
  })
  it('offers a separate model for non-vision providers and saves a picked local model', async () => {
    setup({ vision: false, label: 'DeepSeek' })
    const selection = {
      provider: 'local',
      model: 'gemma4-12b-it',
      quantization: 'Q4_K_XL',
      vision: true,
      reasoning: false,
    } as const
    vi.mocked(commands.getReadingModels).mockResolvedValue([
      {
        provider: 'local',
        model: selection.model,
        name: 'Gemma',
        vision: true,
        reasoning: false,
        quantizations: [{ id: selection.quantization, name: 'Q4', downloaded: false }],
      },
    ])
    mount()
    await clickRead()
    expect(await screen.findByText(/DeepSeek has no vision capabilities/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Yes' }))
    fireEvent.click(await screen.findByRole('button', { name: /Gemma/ }))
    fireEvent.click(await screen.findByRole('button', { name: 'Q4' }))
    fireEvent.click(screen.getByRole('button', { name: 'Run' }))
    await waitFor(() =>
      expect(commands.saveContextSettings).toHaveBeenCalledWith(
        expect.objectContaining({ read_model: selection }),
      ),
    )
    expect(commands.readChapter).toHaveBeenCalledWith({ scope: 'project' }, selection)
  })
  it('skips confirmation only for vision models when asking is off', async () => {
    const info = setup()
    queryClient.setQueryData(contextSettingsKey, {
      read_when: 'missing',
      send_images: false,
      ask_before_reading: false,
      read_model: null,
      pages_per_request: 20,
      previous_pages: 1,
      agent_messages: 20,
    })
    mount()
    await clickRead()
    await waitFor(() =>
      expect(commands.readChapter).toHaveBeenCalledWith({ scope: 'project' }, info.selection),
    )
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })
  it('keeps the Context panel short and moves the brief editor to View', async () => {
    setup()
    mount(<ContextPanel />)
    await screen.findByRole('button', { name: 'View' })
    expect(screen.queryByRole('textbox', { name: 'Chapter brief' })).not.toBeInTheDocument()
    expect(screen.queryByRole('checkbox', { name: /text only/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Check glossary' })).not.toBeInTheDocument()
    expect(screen.queryByText('Flags')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'View' }))
    expect(await screen.findByRole('textbox', { name: 'Chapter brief' })).toBeInTheDocument()
  })
  it('opens the series step before reading an unlinked chapter', async () => {
    setup()
    vi.mocked(commands.getChapterContext).mockResolvedValue({
      context: { version: 1, series: null, brief: null, suggestions: [], lines: {} },
      series: null,
      supports_tasks: false,
    })
    mount()
    await clickRead()
    expect(await screen.findByRole('button', { name: 'New series' })).toBeInTheDocument()
    expect(commands.readChapter).not.toHaveBeenCalled()
  })
  it('restores confirmation in settings and keeps the legacy Always mode as automatic Yes', async () => {
    setup()
    queryClient.setQueryData(contextSettingsKey, {
      read_when: 'always',
      send_images: false,
      ask_before_reading: false,
      read_model: null,
      pages_per_request: 20,
      previous_pages: 1,
      agent_messages: 20,
    })
    mount(<ContextPreferences />)
    expect(
      await screen.findByRole('combobox', { name: 'Automatic chapter reading' }),
    ).toHaveTextContent('Yes')
    expect(screen.queryByText('Always')).not.toBeInTheDocument()
    expect(screen.queryByRole('checkbox', { name: /send page images/i })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('checkbox', { name: 'Ask before reading a chapter' }))
    await waitFor(() =>
      expect(commands.saveContextSettings).toHaveBeenCalledWith(
        expect.objectContaining({ ask_before_reading: true }),
      ),
    )
  })
})
