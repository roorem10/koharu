import { act } from '@testing-library/react'
import { vi } from 'vitest'

import { useBooharuQc } from '@/lib/booharu/qc'
import { commands, events, type Finding, type Problem } from '@koharu/ipc/protocol'

/** The id of the check that finds `problem`, as the desktop names it. */
function checkOf(problem: Problem): string {
  if (problem.kind === 'glossary') return `glossary:${problem.source}`
  return problem.kind
}

/** A finding on `layer` of `page`, open, as the desktop tells one: `problem`, overflow unless given,
 * with `fields` over it. */
export function finding(
  page: string,
  layer: string,
  problem: Problem = { kind: 'overflow' },
  fields: Partial<Finding> = {},
): Finding {
  return {
    page,
    layer,
    check: checkOf(problem),
    problem,
    source: problem.kind === 'flag' ? 'agent' : 'auto',
    text: `${layer} text`,
    hidden: false,
    ...fields,
  }
}

/** Each event's handlers, as the interface listened. */
const handlers = new Map<string, ((payload: unknown) => void)[]>()

/** Listens to the desktop's events as the interface would, keeping each handler for `tell`. */
export function listenToEvents(): void {
  handlers.clear()
  for (const name of Object.keys(events) as (keyof typeof events)[]) {
    vi.spyOn(events, name).mockImplementation(async (handler: (payload: never) => void) => {
      handlers.set(name, [...(handlers.get(name) ?? []), handler as (payload: unknown) => void])
      return () => undefined
    })
  }
}

/** Tells `payload` as the desktop would. */
export function tell(name: keyof typeof events, payload: unknown): void {
  act(() => {
    for (const handler of handlers.get(name) ?? []) handler(payload)
  })
}

/** Lets the reads under way answer. */
export async function settle(): Promise<void> {
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)))
}

/** The desktop with every command answering null, the project `open` open, and each project's
 * findings as `findings` holds them, which `booharu_get_qc` answers. */
export function qcDesktop(open: string | null, findings: Record<string, Finding[]> = {}) {
  const table = commands as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>
  for (const name of Object.keys(table)) vi.spyOn(table, name).mockResolvedValue(null)
  const desk = { open, findings }
  const read = vi
    .spyOn(commands, 'booharuGetQc')
    .mockImplementation(async () => structuredClone(desk.findings[desk.open ?? ''] ?? []))
  vi.spyOn(commands, 'getOpenProject').mockImplementation(async () => desk.open)
  return { desk, read }
}

/** The store as a fresh interface starts it. */
export function forgetFindings(): void {
  useBooharuQc.setState({ project: null, pages: {}, previous: null })
}
