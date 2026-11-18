import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { StoryPanel } from '@/components/editor/StoryPanel'
import { setTranslation } from '@/lib/booharu/translation'
import { pagesKey, projectKey, queryClient, type ProjectView } from '@/lib/queries'
import { commands, type Change, type Edit, type Layer, type Page } from '@koharu/ipc/protocol'

/** A block as detection, or the user's box, leaves it: its source, and `translation` if it has
 * one. */
function block(id: string, source: string, translation?: string): Layer {
  return {
    id,
    name: id,
    visible: true,
    locked: false,
    opacity: 1,
    blend: 'normal',
    attributes: {},
    kind: 'block',
    source: { content: [{ insert: source }] },
    ...(translation === undefined ? {} : { translation: { content: [{ insert: translation }] } }),
  }
}

const page: Page = {
  id: 'page',
  size: { width: 400, height: 400 },
  background: null,
  layers: [block('empty', ''), block('translated', 'こんにちは', 'Hello')],
}

const change: Change = {
  revision: 'r1',
  created: [],
  pages: { added: [], changed: ['page'], removed: [] },
  layers: ['empty'],
  history: { steps: ['booharu_set_translation'], current: 1 },
} as unknown as Change

/** Every command answered without the desktop: the edits and typed translations sent are kept. */
function desktop() {
  const table = commands as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>
  for (const name of Object.keys(table)) vi.spyOn(table, name).mockResolvedValue(null)
  const edits: Edit[] = []
  vi.spyOn(commands, 'edit').mockImplementation(async (edit) => {
    edits.push(edit)
    return change
  })
  const typed = vi.spyOn(commands, 'booharuSetTranslation').mockResolvedValue(change)
  return { edits, typed }
}

function story() {
  const project: ProjectView = {
    name: 'Demo',
    page: 'page',
    pages: [{ id: page.id, size: page.size, background: null }],
    history: { steps: [], current: 0 },
    revisions: {},
  }
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData([...pagesKey, page.id], { page, story: ['empty', 'translated'] })
  render(
    <QueryClientProvider client={queryClient}>
      <StoryPanel />
    </QueryClientProvider>,
  )
}

describe('Booharu typed translation (#128)', () => {
  it('sends the text to the desktop command that places it', async () => {
    const { typed } = desktop()

    await expect(setTranslation('empty', 'Hi')).resolves.toBe(change)
    expect(typed).toHaveBeenCalledWith('empty', 'Hi')
  })

  it("places a block's first translation, and edits one it has as Koharu does", async () => {
    const { edits, typed } = desktop()
    story()
    const user = userEvent.setup()

    // A box drawn by hand, with no source text, then one translated before.
    const [empty, translated] = await screen.findAllByPlaceholderText('Translation')
    await user.type(empty, 'Hi')
    await user.tab()
    expect(typed).toHaveBeenCalledTimes(1)
    expect(typed).toHaveBeenCalledWith('empty', 'Hi')
    expect(edits).toEqual([])

    await user.type(translated, '!')
    await user.tab()
    expect(typed).toHaveBeenCalledTimes(1)
    expect(edits).toEqual([
      { label: 'set_text', ops: [{ op: 'set_text', layer: 'translated', text: 'Hello!' }] },
    ])
  })
})
