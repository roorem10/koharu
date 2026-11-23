import { act } from '@testing-library/react'
import { vi } from 'vitest'

import { projectKey, queryClient, type ProjectView } from '@/lib/queries'
import { commands, type PageSummary } from '@koharu/ipc/protocol'

/** Every command answered without the desktop. */
export function desktop(): void {
  const table = commands as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>
  for (const name of Object.keys(table)) vi.spyOn(table, name).mockResolvedValue(null)
}

/** The project open as a chapter of `count` pages, `p1` to `p<count>`, the canvas showing `shown`.
 * No reading of the Gallery's goes with it, so the reader turns its pages right to left, one at a
 * time while it's no wider than tall, as jsdom measures nothing. */
export function openChapter(count: number, shown: string): void {
  const pages: PageSummary[] = Array.from({ length: count }, (_, index) => ({
    id: `p${index + 1}`,
    size: { width: 800, height: 1200 },
    background: null,
    texts: 0,
    translated: 0,
  }))
  const project: ProjectView = {
    name: 'Chapter',
    page: shown,
    pages,
    history: { steps: [], current: 0 },
    revisions: {},
  }
  queryClient.setQueryData(projectKey, project)
}

/** Lets the commands sent reach their spies: Koharu's `send()` and Booharu's `call()` run each as
 * a mutation, which calls its command a few promises later. */
export function settle(): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)))
}
