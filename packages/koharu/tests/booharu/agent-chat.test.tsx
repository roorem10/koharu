import { QueryClientProvider } from '@tanstack/react-query'
import type { Channel } from '@tauri-apps/api/core'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { AgentPanel } from '@/components/editor/AgentPanel'
import { useConversation } from '@/lib/agent'
import {
  chattedSinceDivider,
  rejoined,
  useAgentChat,
  useBooharuAgentChatEvents,
  withDivider,
  type SavedTurn,
} from '@/lib/booharu/agentChat'
import { hear } from '@/lib/events'
import { resources, supportedLanguages } from '@/lib/i18n'
import { modelsKey, preferencesKey, projectKey, queryClient, type ProjectView } from '@/lib/queries'
import { commands, events, type AgentEntry, type AgentEvent } from '@koharu/ipc/protocol'

import { gpt, koharuPreferences, models } from './reading'

type User = ReturnType<typeof userEvent.setup>

/** What the user asked first, and the agent's answer, as the project's agent file keeps them. */
const asked: AgentEntry = { role: 'user', id: 'u1', at: 1, text: 'Translate page 1' }
const answered: AgentEntry = {
  role: 'assistant',
  id: 'a1',
  at: 2,
  model: gpt.reference,
  state: 'done',
  error: null,
  parts: [{ type: 'text', text: 'Page 1 is translated.' }],
}
/** Explain this line's answer, which the desktop adds to the file. */
const note: AgentEntry = {
  role: 'note',
  id: 'n1',
  at: 3,
  kind: 'explain',
  layer: 'free',
  source: '潔の武器',
  translation: "Isagi's weapon",
  explanation: '武器 is his weapon: the skill he means to score with.',
  alternatives: ["Isagi's edge", "Isagi's secret weapon"],
  model: gpt.reference,
}

/** Each event's handlers, as the interface listened. */
const handlers = new Map<string, ((payload: unknown) => void)[]>()

/** Tells `payload` as the desktop would. */
function tell(name: keyof typeof events, payload: unknown): void {
  act(() => {
    for (const handler of handlers.get(name) ?? []) handler(payload)
  })
}

/** The project `name` open, as the desktop tells it. */
function openProject(name: string): void {
  const project: ProjectView = {
    name,
    page: null,
    pages: [],
    history: { steps: [], current: 0 },
    revisions: {},
  }
  queryClient.setQueryData(projectKey, project)
}

/** Every command answered without the desktop, each project's agent file as `files` has it; each
 * run's channel kept, its id one more than the last. */
function chatDesktop(files: Record<string, AgentEntry[]>) {
  const table = commands as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>
  for (const name of Object.keys(table)) vi.spyOn(table, name).mockResolvedValue(null)
  const desk = { open: 'Demo', files, channels: [] as Channel<AgentEvent>[] }
  const history = vi
    .spyOn(commands, 'booharuGetAgentHistory')
    .mockImplementation(async () => structuredClone(desk.files[desk.open] ?? []))
  const run = vi.spyOn(commands, 'runAgent').mockImplementation(async (_message, channel) => {
    desk.channels.push(channel)
    return desk.channels.length
  })
  const stop = vi.spyOn(commands, 'stopAgent').mockResolvedValue(null)
  const reset = vi.spyOn(commands, 'resetAgent').mockResolvedValue(null)
  openProject(desk.open)
  return { desk, history, run, stop, reset }
}

/** Tells the run `run`'s panel `update`, as the desktop sends it on the run's channel. */
function send(desk: { channels: Channel<AgentEvent>[] }, run: number, update: AgentEvent): void {
  act(() => desk.channels[run - 1]!.onmessage(update))
}

/** The agent's panel as the right pane holds it, one for every project, as Koharu's conversation
 * is kept apart from it. */
function Chat() {
  return <AgentPanel />
}

/** The app's listener for the agent file's changes and the project's, as the providers mount it. */
function Events() {
  useBooharuAgentChatEvents()
  return null
}

/** Lets the reads under way answer. */
async function settle(): Promise<void> {
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)))
}

function mount(ui: ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>)
}

/** Sends `message` from the composer. */
async function ask(user: User, message: string) {
  await user.type(screen.getByRole('textbox', { name: 'Message Booharu Agent' }), message)
  await user.click(screen.getByRole('button', { name: 'Send' }))
}

beforeEach(() => {
  handlers.clear()
  for (const name of Object.keys(events) as (keyof typeof events)[]) {
    vi.spyOn(events, name).mockImplementation(async (handler: (payload: never) => void) => {
      handlers.set(name, [...(handlers.get(name) ?? []), handler as (payload: unknown) => void])
      return () => undefined
    })
  }
  // A run's channel registers its callback with the webview.
  vi.stubGlobal('__TAURI_INTERNALS__', { transformCallback: () => 1, unregisterCallback: vi.fn() })
  useAgentChat.setState({ project: null, stale: false })
  useConversation.setState(useConversation.getInitialState())
  queryClient.setQueryData(modelsKey, models)
  queryClient.setQueryData(preferencesKey, {
    ...koharuPreferences(gpt.reference),
    agent: { model: gpt.reference },
  })
})

afterEach(() => vi.unstubAllGlobals())

describe("Booharu agent's chat", () => {
  it('keeps the chat and the run going while the panel is away, as Settings takes it', async () => {
    const { desk, history, run, stop } = chatDesktop({ Demo: [asked, answered] })
    const user = userEvent.setup()
    let view = mount(<Chat />)

    expect(await screen.findByText('Translate page 1')).toBeInTheDocument()
    expect(screen.getByText('Page 1 is translated.')).toBeInTheDocument()
    await ask(user, 'Letter page 2')
    expect(run).toHaveBeenCalledWith('Letter page 2', desk.channels[0])
    send(desk, 1, { type: 'event', event: { type: 'text', delta: 'Lettering page 2' } })
    expect(screen.getByText('Lettering page 2')).toBeInTheDocument()

    // The run ends while the panel is away.
    view.unmount()
    send(desk, 1, { type: 'completed', answer: 'Lettering page 2' })
    view = mount(<Chat />)
    expect(screen.getByText('Translate page 1')).toBeInTheDocument()
    expect(screen.getByText('Letter page 2')).toBeInTheDocument()
    expect(screen.getByText('Lettering page 2')).toBeInTheDocument()
    expect(screen.queryByText('Working...')).toBeNull()

    // Stop still stops the run going once the panel is back.
    await ask(user, 'Clean page 3')
    view.unmount()
    view = mount(<Chat />)
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(stop).toHaveBeenCalledWith(2)
    expect(screen.getByText('Cancelled.')).toBeInTheDocument()
    // The file is read once, as the project opened.
    expect(history).toHaveBeenCalledTimes(1)
  })

  it('starts over as another project opens, with that project’s chat', async () => {
    const other: AgentEntry = { role: 'user', id: 'u9', at: 9, text: 'Check the other book' }
    const { desk, history } = chatDesktop({ Demo: [asked, answered], Other: [other] })
    mount(
      <>
        <Events />
        <Chat />
      </>,
    )
    expect(await screen.findByText('Translate page 1')).toBeInTheDocument()
    // The other project's file answers once it's let.
    let read: () => void = () => undefined
    history.mockImplementationOnce(
      () =>
        new Promise((resolve) => (read = () => resolve(structuredClone(desk.files[desk.open]!)))),
    )

    // The desktop closes the open project, then opens the other, the panel staying.
    desk.open = 'Other'
    tell('project', null)
    tell('project', 'Other')

    // Nothing of the project before shows, even before the other's chat is read.
    expect(useAgentChat.getState().project).toBe('Other')
    expect(useConversation.getState()).toMatchObject({ turns: [], running: null })
    act(() => openProject('Other'))
    expect(screen.queryByText('Translate page 1')).toBeNull()
    // The read runs as a mutation does, a few promises after the project opened.
    await waitFor(() => expect(history).toHaveBeenCalledTimes(2))
    act(() => read())
    expect(await screen.findByText('Check the other book')).toBeInTheDocument()
    expect(history).toHaveBeenCalledTimes(2)
  })

  it('keeps the draft while Read takes the panel, and starts over as the project closes, reading the chat once as it opens again', async () => {
    const { desk, history } = chatDesktop({ Demo: [asked, answered] })
    // The project read again as the desktop tells it open, as Koharu reads it.
    let open: string | null = desk.open
    vi.spyOn(commands, 'getOpenProject').mockImplementation(async () => open)
    vi.spyOn(commands, 'query').mockImplementation(async ({ query }) =>
      query === 'get_project'
        ? { id: 'project', version: '1', pages: [] }
        : { steps: [], current: 0 },
    )
    vi.spyOn(commands, 'getPipelineEvents').mockResolvedValue([])
    // Koharu's listeners and Booharu's, as the providers mount them.
    void hear(() => true)
    const user = userEvent.setup()
    mount(<Events />)
    const view = mount(<Chat />)
    await screen.findByText('Page 1 is translated.')
    await ask(user, 'Letter page 2')
    await waitFor(() => expect(useConversation.getState().running).toBe(1))
    await user.type(screen.getByRole('textbox', { name: 'Message Booharu Agent' }), 'Clean page 3')

    // Read takes the panel's place, and gives it back.
    view.unmount()
    mount(<Chat />)
    expect(screen.getByRole('textbox', { name: 'Message Booharu Agent' })).toHaveValue(
      'Clean page 3',
    )
    expect(screen.getByText('Letter page 2')).toBeInTheDocument()

    // The desktop closes the project, Koharu's listener and Booharu's both hearing it.
    open = null
    tell('project', null)
    expect(useConversation.getState()).toEqual(useConversation.getInitialState())
    expect(useAgentChat.getState()).toEqual({ project: null, stale: false })
    expect(screen.queryByText('Translate page 1')).toBeNull()
    expect(screen.getByRole('textbox', { name: 'Message Booharu Agent' })).toHaveValue('')
    // The run's end, told once the project closed, shows nothing.
    send(desk, 1, { type: 'completed', answer: 'Page 2 is lettered.' })
    expect(screen.queryByText('Page 2 is lettered.')).toBeNull()

    // It opens again, the desktop having saved the turn.
    desk.files.Demo = [
      asked,
      answered,
      { role: 'user', id: 'u2', at: 6, text: 'Letter page 2' },
      { ...answered, id: 'a2', at: 7, parts: [{ type: 'text', text: 'Page 2 is lettered.' }] },
    ]
    open = 'Demo'
    tell('project', 'Demo')
    expect(await screen.findByText('Page 2 is lettered.')).toBeInTheDocument()
    await waitFor(() =>
      expect(queryClient.getQueryData(projectKey)).toMatchObject({ name: 'Demo' }),
    )
    await settle()
    for (const said of ['Translate page 1', 'Page 1 is translated.', 'Letter page 2'])
      expect(screen.getAllByText(said), said).toHaveLength(1)
    expect(screen.getAllByText('Page 2 is lettered.')).toHaveLength(1)
    expect(history).toHaveBeenCalledTimes(2)
    expect(useAgentChat.getState().project).toBe('Demo')
  })

  it('keeps what was said above a divider on New chat', async () => {
    const { desk, reset } = chatDesktop({ Demo: [asked, answered] })
    const user = userEvent.setup()
    mount(
      <>
        <Events />
        <Chat />
      </>,
    )
    const said = await screen.findByText('Page 1 is translated.')

    await user.click(screen.getByRole('button', { name: 'New chat' }))

    expect(reset).toHaveBeenCalledTimes(1)
    const divider = await screen.findByText('Previous conversation')
    expect(said).toBeInTheDocument()
    expect(said.compareDocumentPosition(divider) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    // Nothing said since, so there's nothing for New chat to divide.
    expect(screen.getByRole('button', { name: 'New chat' })).toBeDisabled()

    // The desktop wrote its own divider, which takes the panel's place.
    desk.files.Demo = [asked, answered, { role: 'divider', id: 'd1', at: 3, reason: 'new_chat' }]
    tell('booharuAgentHistoryChanged', null)
    await waitFor(() =>
      expect(useConversation.getState().turns.map(({ id }) => id)).toContain('d1'),
    )
    expect(screen.getAllByText('Previous conversation')).toHaveLength(1)
    expect(screen.getByText('Translate page 1')).toBeInTheDocument()
  })

  it('reads the file again as it changes, once no turn runs', async () => {
    const { desk, history } = chatDesktop({ Demo: [asked, answered] })
    const user = userEvent.setup()
    mount(
      <>
        <Events />
        <Chat />
      </>,
    )
    await screen.findByText('Page 1 is translated.')

    // Explain this line answered while the chat sat idle.
    desk.files.Demo = [asked, answered, note]
    tell('booharuAgentHistoryChanged', null)
    expect(await screen.findByRole('article', { name: 'Explained a line' })).toBeInTheDocument()
    expect(history).toHaveBeenCalledTimes(2)

    // Another answer lands while a turn runs: the file waits for the turn's end.
    await ask(user, 'Letter page 2')
    const later = { ...note, id: 'n2', at: 5, explanation: 'Ego is the theme.' }
    desk.files.Demo = [...desk.files.Demo, later]
    tell('booharuAgentHistoryChanged', null)
    expect(history).toHaveBeenCalledTimes(2)
    expect(screen.queryByText('Ego is the theme.')).toBeNull()

    // The desktop writes the turn, then the run ends.
    desk.files.Demo = [
      ...desk.files.Demo,
      { role: 'user', id: 'u2', at: 6, text: 'Letter page 2' },
      { ...answered, id: 'a2', at: 7, parts: [{ type: 'text', text: 'Page 2 is lettered.' }] },
    ]
    send(desk, 1, { type: 'completed', answer: 'Page 2 is lettered.' })

    expect(await screen.findByText('Ego is the theme.')).toBeInTheDocument()
    expect(history).toHaveBeenCalledTimes(3)
    expect(screen.getAllByText('Page 2 is lettered.')).toHaveLength(1)
    expect(screen.getAllByText('Letter page 2')).toHaveLength(1)
  })

  it('shows Explain this line’s answers, and why a conversation started afresh', async () => {
    chatDesktop({
      Demo: [asked, answered, note, { role: 'divider', id: 'd1', at: 4, reason: 'memory_lost' }],
    })
    mount(<Chat />)

    const card = within(await screen.findByRole('article', { name: 'Explained a line' }))
    expect(card.getByText('潔の武器')).toBeInTheDocument()
    expect(card.getByText("Isagi's weapon")).toBeInTheDocument()
    expect(card.getByText(note.explanation)).toBeInTheDocument()
    expect(
      within(card.getByRole('list', { name: 'Other translations' }))
        .getAllByRole('listitem')
        .map((item) => item.textContent),
    ).toEqual(note.alternatives)
    expect(card.getByText('Answered by GPT-5.4')).toBeInTheDocument()
    expect(screen.getByText('Previous conversation')).toBeInTheDocument()
    expect(
      screen.getByText(
        "The agent couldn't read back what it remembered of the conversation above, so it starts afresh.",
      ),
    ).toBeInTheDocument()
    // Nothing was said since the model's memory was lost.
    expect(screen.getByRole('button', { name: 'New chat' })).toBeDisabled()
  })

  it("keeps the panel's ids for the turns the file holds, in order", () => {
    const shown = [
      { id: 'mine-1', role: 'user', text: 'Hi' },
      { id: 'mine-2', role: 'assistant' },
      { id: 'mine-3', role: 'user', text: 'Hi' },
    ]
    const saved: SavedTurn[] = [
      { id: 'u1', role: 'user', text: 'Hi' },
      { id: 'a1', role: 'assistant', parts: [], state: 'done' },
      { id: 'd1', role: 'divider', at: 3, reason: 'new_chat' },
      { id: 'u2', role: 'user', text: 'Hi' },
      { id: 'u3', role: 'user', text: 'Bye' },
    ]
    expect(rejoined(shown, saved).map(({ id }) => id)).toEqual([
      'mine-1',
      'mine-2',
      'd1',
      'mine-3',
      'u3',
    ])
  })

  it('divides only a chat with something said since its last divider, notes aside', () => {
    const said: SavedTurn[] = [{ id: 'u1', role: 'user', text: 'Hi' }]
    const divided = withDivider(said)
    expect(divided.map(({ role }) => role)).toEqual(['user', 'divider'])
    expect(divided.at(-1)).toMatchObject({ role: 'divider', reason: 'new_chat' })
    // Nothing said since: no second divider, Explain this line's notes not counting.
    expect(withDivider(divided)).toBe(divided)
    const noted = [...divided, { ...(note as Extract<SavedTurn, { role: 'note' }>) }]
    expect(chattedSinceDivider(noted)).toBe(false)
    expect(withDivider(noted)).toBe(noted)
    expect(chattedSinceDivider([...noted, { id: 'a1', role: 'assistant' }])).toBe(true)
  })

  it('has its strings in all 12 languages, each filling in what English does', () => {
    const keys = [
      'booharu.agent.divider',
      'booharu.agent.memoryLost',
      'booharu.agent.explained',
      'booharu.context.alternatives',
      'booharu.context.answeredBy',
    ]
    const text = (language: string, key: string) =>
      key
        .split('.')
        .reduce<unknown>(
          (node, part) => (node as Record<string, unknown> | undefined)?.[part],
          resources[language].translation,
        )
    /** What a string fills in: its `{{values}}`. */
    const fills = (value: unknown) =>
      typeof value === 'string' ? (value.match(/{{\w+}}/g) ?? []).sort() : null
    expect(supportedLanguages).toHaveLength(12)
    for (const language of supportedLanguages) {
      for (const key of keys) {
        const value = text(language, key)
        expect(typeof value === 'string' && value.trim() !== '', `${language}: ${key}`).toBe(true)
        expect(fills(value), `${language}: ${key}`).toEqual(fills(text('en-US', key)))
      }
    }
  })
})
