import { QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { CanvasWorkspace } from '@/components/editor/CanvasWorkspace'
import { isTool, mergeWorkspace } from '@/lib/booharu/workspace'
import { queryClient } from '@/lib/queries'
import { toolKeys } from '@/lib/shortcuts'
import { useKoharuStore } from '@/lib/store'
import { commands, type Tool } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

/** Koharu's workspace as a test build of 0.0.5 kept it, its O tool picked. */
const keptWithTheOTool = {
  state: {
    tool: 'ocr_box',
    brush: { diameter: 30, color: '#000000' },
    processingScope: 'selected-pages',
    processingStages: ['detect', 'recognize'],
    galleryOpen: true,
  },
  version: 0,
}

/** Lets what the workspace sent as it mounted reach the spies, a few promises later. */
function settle(): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)))
}

afterEach(() => {
  cleanup()
  localStorage.clear()
})

describe("Koharu's kept workspace", () => {
  it("leaves out a kept tool the app doesn't have, as the O tool, and keeps the rest", async () => {
    localStorage.setItem('koharu.workspace', JSON.stringify(keptWithTheOTool))
    await useKoharuStore.persist.rehydrate()
    const state = useKoharuStore.getState()
    expect(state.tool).toBe('select')
    expect(state.brush).toEqual({ diameter: 30, color: '#000000' })
    expect(state.processingScope).toBe('selected-pages')
    expect(state.processingStages).toEqual(['detect', 'recognize'])
    expect(state.galleryOpen).toBe(true)
  })

  it('brings back each tool the app has, K among them', async () => {
    for (const tool of Object.keys(toolKeys) as Tool[]) {
      useKoharuStore.setState({ tool: 'select' })
      localStorage.setItem('koharu.workspace', JSON.stringify({ state: { tool }, version: 0 }))
      await useKoharuStore.persist.rehydrate()
      expect(useKoharuStore.getState().tool).toBe(tool)
    }
  })

  it("keeps the store's own tool for a kept one that names no tool, or none kept", () => {
    const current = { tool: 'eraser' as Tool, open: false }
    for (const tool of ['ocr_box', '', 'Select', 'toString', 'constructor', 42, null, {}]) {
      expect(mergeWorkspace({ tool, open: true }, current), String(tool)).toEqual({
        tool: 'eraser',
        open: true,
      })
      expect(isTool(tool), String(tool)).toBe(false)
    }
    expect(mergeWorkspace({ open: true }, current)).toEqual({ tool: 'eraser', open: true })
    expect(mergeWorkspace(null, current)).toBe(current)
    expect(mergeWorkspace('select', current)).toBe(current)
  })

  it('sends the canvas only a tool it has as the editor opens, with no error shown', async () => {
    localStorage.setItem('koharu.workspace', JSON.stringify(keptWithTheOTool))
    await useKoharuStore.persist.rehydrate()
    const tools: unknown[] = []
    const table = commands as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>
    for (const name of Object.keys(table)) {
      vi.spyOn(table, name).mockImplementation(async (...args: unknown[]) => {
        // As the desktop answers a tool it can't read: Tauri's invalid args.
        if (name === 'setTool') {
          tools.push(args[0])
          if (!isTool(args[0])) throw 'invalid args `tool` for command `set_tool`'
        }
        return null
      })
    }
    // jsdom has no 2D canvas; the workspace then sends black for its theme colours.
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null)
    const shown = vi.spyOn(toast, 'add')
    render(
      <QueryClientProvider client={queryClient}>
        <CanvasWorkspace />
      </QueryClientProvider>,
    )
    await settle()
    await settle()
    expect(tools).toEqual(['select'])
    expect(shown).not.toHaveBeenCalled()
  })
})
