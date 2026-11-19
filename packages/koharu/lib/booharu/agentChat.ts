'use client'

import { useCallback, useEffect } from 'react'
import { create } from 'zustand'

import { call } from '@/lib/backend'
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

/** The open project's conversation with the agent as the chat panel shows it (D18): held apart from
 * the panel, which Settings unmounts, so the chat, and Stop for the run going, outlive it, while the
 * desktop's conversation goes on. */
interface AgentChat {
  /** The project whose chat it holds; none while none is open. */
  project: string | null
  turns: ChatTurn[]
  /** The run going, which Stop stops. */
  running: number | null
  /** Whether the file changed while a turn ran, so it's read again once none does. */
  stale: boolean
}

const empty = { turns: [], running: null, stale: false }

export const useAgentChat = create<AgentChat>()(() => ({ project: null, ...empty }))

/** The turns of a project the store doesn't hold. */
const none: ChatTurn[] = []

/** Holds `project`'s chat: as it is when the store holds that project's already, else read from
 * its agent file, none until the file is read. */
function hold(project: string | null): void {
  if (useAgentChat.getState().project === project) return
  useAgentChat.setState({ project, ...empty })
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
 * that doesn't read shows its error, as every failed command's does, and leaves the chat as it is. */
async function reread(project: string): Promise<void> {
  if (useAgentChat.getState().turns.some(running)) {
    useAgentChat.setState((state) => (state.project === project ? { stale: true } : state))
    return
  }
  const read = ++reads
  const entries = await call(commands.booharuGetAgentHistory).catch(() => null)
  if (!Array.isArray(entries)) return
  useAgentChat.setState((state) => {
    if (state.project !== project || read !== reads) return state
    if (state.turns.some(running)) return { stale: true }
    return { turns: rejoined(state.turns, entries.map(turnOf)), stale: false }
  })
}

/** The desktop changed the open project's agent file: a turn ended, New chat was pressed, Explain
 * this line answered, or the model's memory couldn't be read back. Read again, once no turn runs. */
function chatChanged(): void {
  const { project } = useAgentChat.getState()
  // A project the store doesn't hold yet is read as its chat opens.
  if (project !== null) void reread(project)
}

type SetTurns<T> = (update: T[] | ((current: T[]) => T[])) => void

/** The open project's chat, `[turns, setTurns]` as `useState` gives them to the chat panel, whose
 * `Turn` must hold each `SavedTurn`, or this call doesn't compile: kept by project, read from the
 * project's agent file once it opens, and again whenever the desktop tells it changed, once no turn
 * runs, starting over with another project. A turn's setter changes only the project it was made
 * for, so a run's events that come once another project opened change nothing. */
export function useBooharuAgentChat<T extends ChatTurn>(
  // Empty when the panel's `Turn` holds each `SavedTurn`.
  ..._fits: SavedTurn extends T ? [] : [never]
): [T[], SetTurns<T>] {
  const project = useProject().data?.name ?? null
  const turns = useAgentChat((state) => (state.project === project ? state.turns : none))
  useEffect(() => hold(project), [project])
  const setTurns = useCallback<SetTurns<T>>(
    (update) => {
      useAgentChat.setState((state) => {
        if (state.project !== project) return state
        const current = state.turns as T[]
        return { turns: typeof update === 'function' ? update(current) : update }
      })
      const { stale, turns: now } = useAgentChat.getState()
      if (project !== null && stale && !now.some(running)) void reread(project)
    },
    [project],
  )
  return [turns as T[], setTurns]
}

/** The open project's agent run going, `[running, setRunning]`, as `useState` gives them to the
 * chat panel: kept with its chat, so Stop works after Settings. */
export function useBooharuAgentRun(): [number | null, (run: number | null) => void] {
  const project = useProject().data?.name ?? null
  const running = useAgentChat((state) => (state.project === project ? state.running : null))
  const setRunning = useCallback(
    (run: number | null) =>
      useAgentChat.setState((state) => (state.project === project ? { running: run } : state)),
    [project],
  )
  return [running, setRunning]
}

/** Keeps the agent's chat with the open project: it starts over as another project opens, or the
 * open one closes, and is read again whenever the desktop tells it changed, whether or not the
 * panel shows, as while Settings is open. Mounted once, in the app's providers. */
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
    return () => {
      active = false
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

/** A line Discuss handed to the agent, which the chat's composer takes once. */
const useAgentDrafts = create<{ draft: { text: string } | null }>()(() => ({ draft: null }))

/** Discuss: puts `prompt` in the agent's composer, in place of what it holds, and shows the Agent
 * tab. Nothing is sent until the user sends it. */
export function discussInAgent(prompt: string): void {
  useAgentDrafts.setState({ draft: { text: prompt } })
  useKoharuStore.getState().showPanel('agent')
}

/** Copies each line Discuss hands over into the chat's composer once, through `fill`, the
 * composer's setter. */
export function useAgentDraft(fill: (text: string) => void): void {
  const draft = useAgentDrafts((state) => state.draft)
  useEffect(() => {
    if (!draft) return
    fill(draft.text)
    useAgentDrafts.setState((state) => (state.draft === draft ? { draft: null } : state))
  }, [draft, fill])
}
