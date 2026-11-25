'use client'

import { useEffect } from 'react'
import { create } from 'zustand'

import { useConversation } from '@/lib/agent'
import { call } from '@/lib/booharu/call'
import { useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  events,
  type AgentEntry,
  type AgentPart,
  type AgentTurnState,
} from '@koharu/ipc/protocol'

/** A row Booharu adds to the agent's chat: where a new conversation starts, which the model
 * remembers nothing above, or Explain this line's answer, which it never heard. */
export type BooharuTurn = Extract<AgentEntry, { role: 'divider' | 'note' }>

/** An entry of the project's agent file as the chat panel holds a turn: the user's message, the
 * agent's answer as the panel folded it, or one of Booharu's rows. */
export type SavedTurn =
  | { id: string; role: 'user'; text: string }
  | { id: string; role: 'assistant'; parts: AgentPart[]; state: AgentTurnState; error?: string }
  | BooharuTurn

/** What the store reads of every turn of the panel's: its id and whose it is. */
interface ChatTurn {
  id: string
  role: string
}

/** Whether `turn` is one of Booharu's rows, which the chat panel draws with `ChatRow`. */
export function isBooharuTurn(turn: ChatTurn): turn is BooharuTurn {
  return turn.role === 'divider' || turn.role === 'note'
}

/** Whether `turn` is an answer still coming. */
function running(turn: ChatTurn): boolean {
  return turn.role === 'assistant' && (turn as { state?: unknown }).state === 'running'
}

/** Whose chat Koharu's conversation holds (D18). Koharu's store (`useConversation`) holds the turns,
 * the draft and the run going apart from the panel, which Settings and Read unmount, and starts over
 * as the project closes; Booharu reads the project's saved chat into it. */
interface AgentChat {
  /** The project whose chat the conversation holds; none while none is open. */
  project: string | null
  /** Whether the file changed while a turn ran, so it's read again once none does. */
  stale: boolean
}

export const useAgentChat = create<AgentChat>()(() => ({ project: null, stale: false }))

/** Holds `project`'s chat: as it is when the conversation holds that project's already, else
 * started over, as Koharu starts it over as a project closes, and read from its agent file, none
 * until the file is read. */
function hold(project: string | null): void {
  if (useAgentChat.getState().project === project) return
  useAgentChat.setState({ project, stale: false })
  useConversation.setState(useConversation.getInitialState())
  if (project !== null) void reread(project)
}

/** `entry` as the chat panel holds a turn. */
function turnOf(entry: AgentEntry): SavedTurn {
  switch (entry.role) {
    case 'user':
      return { id: entry.id, role: 'user', text: entry.text }
    case 'assistant':
      return {
        id: entry.id,
        role: 'assistant',
        parts: entry.parts,
        state: entry.state,
        ...(entry.error === null ? {} : { error: entry.error }),
      }
    default:
      return entry
  }
}

/** `saved`, the file's entries as turns, each message and answer keeping the id of the one the
 * panel shows in its place, matched in order, a message by its text, so the panel keeps its rows as
 * they were, a fold open or closed. The file holds what was said: a message whose run failed before
 * it started, which the desktop never saved, goes. */
export function rejoined(current: readonly ChatTurn[], saved: readonly SavedTurn[]): SavedTurn[] {
  const shown = current.filter(({ role }) => role === 'user' || role === 'assistant')
  let next = 0
  return saved.map((turn) => {
    if (turn.role !== 'user' && turn.role !== 'assistant') return turn
    const at = shown.findIndex(
      (held, index) =>
        index >= next &&
        held.role === turn.role &&
        (turn.role !== 'user' || (held as { text?: unknown }).text === turn.text),
    )
    const held = shown[at]
    if (!held) return turn
    next = at + 1
    return { ...turn, id: held.id }
  })
}

/** Counts the reads of the agent file, so only the last one asked for lands. */
let reads = 0

/** Reads `project`'s agent file into its chat, unless a turn runs, which the read waits for. A file
 * that doesn't read shows its error, as every failed command's does, and leaves the chat as it is.
 * The file's turns take the conversation's place, so none shows twice. */
async function reread(project: string): Promise<void> {
  if (useConversation.getState().turns.some(running)) {
    useAgentChat.setState((state) => (state.project === project ? { stale: true } : state))
    return
  }
  const read = ++reads
  const entries = await call(commands.booharuGetAgentHistory).catch(() => null)
  if (!Array.isArray(entries)) return
  if (useAgentChat.getState().project !== project || read !== reads) return
  if (useConversation.getState().turns.some(running)) {
    useAgentChat.setState({ stale: true })
    return
  }
  useAgentChat.setState({ stale: false })
  useConversation.setState(({ turns }) => ({ turns: rejoined(turns, entries.map(turnOf)) }))
}

/** The desktop changed the open project's agent file: a turn ended, New chat was pressed, Explain
 * this line answered, or the model's memory couldn't be read back. Read again, once no turn runs. */
function chatChanged(): void {
  const { project } = useAgentChat.getState()
  // A project the store doesn't hold yet is read as its chat opens.
  if (project !== null) void reread(project)
}

/** Holds the open project's chat in the agent's conversation, as the chat panel shows: read from
 * the project's agent file once it opens, starting over with another project. */
export function useBooharuAgentChat(): void {
  const project = useProject().data?.name ?? null
  useEffect(() => hold(project), [project])
}

/** Keeps the agent's chat with the open project: it starts over as another project opens, or the
 * open one closes, and is read again whenever the desktop tells it changed, whether or not the
 * panel shows, as while Settings is open, or once no turn runs when one did. Mounted once, in the
 * app's providers. */
export function useBooharuAgentChatEvents(): void {
  useEffect(() => {
    let active = true
    const listening = [
      events.booharuAgentHistoryChanged(() => {
        if (active) chatChanged()
      }),
      events.project((name) => {
        if (active) hold(name)
      }),
    ].map((listener) => listener.catch(() => null))
    // A file that changed while a turn ran is read once none does.
    const ended = useConversation.subscribe(({ turns }) => {
      const { project, stale } = useAgentChat.getState()
      if (!active || project === null || !stale || turns.some(running)) return
      useAgentChat.setState({ stale: false })
      void reread(project)
    })
    return () => {
      active = false
      ended()
      for (const listener of listening) void listener.then((stop) => stop?.())
    }
  }, [])
}

/** Whether a message or an answer came since the last divider of `turns`, Explain this line's
 * notes aside, as the desktop counts what New chat divides. */
export function chattedSinceDivider(turns: readonly ChatTurn[]): boolean {
  for (const { role } of [...turns].reverse()) {
    if (role === 'divider') return false
    if (role === 'user' || role === 'assistant') return true
  }
  return false
}

/** `turns` as New chat leaves them, as the desktop leaves its file: what was said stays, above a
 * divider below which the model remembers nothing; no divider goes below one with nothing said
 * since. */
export function withDivider<T extends ChatTurn>(turns: (T | BooharuTurn)[]): (T | BooharuTurn)[] {
  if (!chattedSinceDivider(turns)) return turns
  return [
    ...turns,
    { id: crypto.randomUUID(), role: 'divider', at: Date.now(), reason: 'new_chat' },
  ]
}

/** Discuss: puts `prompt` in the agent's composer, Koharu's draft, in place of what it holds, and
 * shows the Agent tab. Nothing is sent until the user sends it. */
export function discussInAgent(prompt: string): void {
  useConversation.setState({ prompt })
  useKoharuStore.getState().showPanel('agent')
}
