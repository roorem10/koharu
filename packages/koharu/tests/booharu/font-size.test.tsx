import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { TypeBar } from '@/components/editor/TypeBar'
import { fontsKey, pageKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Edit, type Page } from '@koharu/ipc/protocol'

const page: Page = {
  id: 'page',
  size: { width: 800, height: 1200 },
  background: null,
  layers: [
    {
      id: 'text',
      name: 'Text',
      visible: true,
      locked: false,
      opacity: 1,
      blend: 'normal',
      attributes: {},
      kind: 'text',
      content: [{ insert: 'Hello' }],
      style: { chars: { size: 24 } },
    },
  ],
}

describe('Booharu font size on Enter', () => {
  it('applies a typed size as one edit when Enter is pressed', async () => {
    const edits: Edit[] = []
    const table = commands as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>
    for (const name of Object.keys(table)) vi.spyOn(table, name).mockResolvedValue(null)
    vi.spyOn(commands, 'edit').mockImplementation(async (edit) => {
      edits.push(edit)
      return null as never
    })
    queryClient.setQueryData(pageKey, page)
    queryClient.setQueryData(fontsKey, [])
    useKoharuStore.setState({
      tool: 'select',
      selectedLayers: ['text'],
      selectionBounds: { x: 100, y: 100, width: 200, height: 80 },
    })
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <TypeBar />
      </QueryClientProvider>,
    )

    const size = screen.getByTestId('type-size')
    await user.clear(size)
    await user.type(size, '8')
    expect(edits).toEqual([])
    await user.keyboard('{Enter}')

    expect(size).not.toHaveFocus()
    expect(edits).toEqual([
      {
        label: 'set_style',
        ops: [{ op: 'update_layer', layer: 'text', patch: { style: { chars: { size: 8 } } } }],
      },
    ])
  })
})
