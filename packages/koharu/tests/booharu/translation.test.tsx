import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { StoryPanel } from '@/components/editor/StoryPanel'
import { isPlaceholder, setTranslation } from '@/lib/booharu/translation'
import { pagesKey, projectKey, queryClient, type ProjectView } from '@/lib/queries'
import {
  commands,
  type Change,
  type Edit,
  type Layer,
  type Page,
  type Text,
} from '@koharu/ipc/protocol'

/** A block as detection, or the user's box, leaves it: its source, and `translation` if it has
 * one. */
function block(id: string, source: string, translation?: string | Text): Layer {
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
    ...(translation === undefined
      ? {}
      : {
          translation:
            typeof translation === 'string' ? { content: [{ insert: translation }] } : translation,
        }),
  }
}

const bounds = { x: 0, y: 0, width: 120, height: 60 }

/** What the type bar leaves on a block styled before it's translated: its source's blank, in its
 * source's frame (Koharu's #1494), with the style merged in. */
const placeholder: Text = {
  content: [],
  style: { chars: { size: 30 } },
  frame: { box: { bounds } },
  direction: 'horizontal',
  language: 'und',
}

const page: Page = {
  id: 'page',
  size: { width: 400, height: 400 },
  background: null,
  layers: [
    block('empty', ''),
    block('translated', 'こんにちは', 'Hello'),
    block('styled', 'やあ', placeholder),
    // Translated, then its text cleared in Story: blank in its frame, as a placeholder is.
    block('cleared', 'またね', { content: [], frame: { box: { bounds } } }),
  ],
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
    pages: [{ id: page.id, size: page.size, background: null, texts: 0, translated: 0 }],
    history: { steps: [], current: 0 },
    revisions: {},
  }
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData([...pagesKey, page.id], {
    page,
    story: ['empty', 'translated', 'styled', 'cleared'],
  })
  render(
    <QueryClientProvider client={queryClient}>
      <StoryPanel />
    </QueryClientProvider>,
  )
}

describe('Booharu placeholder translation (P-d)', () => {
  it('is blank text in whatever frame its source gave it, and not no translation or text', () => {
    expect(isPlaceholder(undefined)).toBe(false)
    expect(isPlaceholder(null)).toBe(false)
    expect(isPlaceholder(placeholder)).toBe(true)
    // A frame tells nothing since the scene starts it in its source's (#1494): point, path or none.
    expect(isPlaceholder({ ...placeholder, frame: 'point' })).toBe(true)
    expect(isPlaceholder({ content: [], frame: { path: { path: 'M0 0 L9 0 L9 9 Z' } } })).toBe(true)
    expect(isPlaceholder({})).toBe(true)
    // Whitespace as the desktop trims it, NEL among it, which JavaScript's trim keeps, and
    // U+FEFF not, which JavaScript's trim takes.
    const blank = [{ insert: ' \n' }, { insert: '\u3000\u0085' }]
    expect(isPlaceholder({ ...placeholder, content: blank })).toBe(true)
    expect(isPlaceholder({ ...placeholder, content: [{ insert: '\uFEFF' }] })).toBe(false)
    expect(isPlaceholder({ ...placeholder, content: [{ insert: ' Hi ' }] })).toBe(false)
  })
})

describe('Booharu typed translation', () => {
  it('sends the text to the desktop command that writes a first translation', async () => {
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

  it('writes the first translation typed into a block styled before it was translated, and leaves a cleared one to the desktop (P-d)', async () => {
    const { edits, typed } = desktop()
    story()
    const user = userEvent.setup()

    // Story names a row by its translation, which these blank ones leave unnamed.
    const [, , styled, cleared] = await screen.findAllByPlaceholderText('Translation')
    await user.type(styled, 'Hi')
    await user.tab()
    expect(typed).toHaveBeenCalledTimes(1)
    expect(typed).toHaveBeenCalledWith('styled', 'Hi')
    expect(edits).toEqual([])

    // Only the block's record tells a run's translation emptied by hand from a placeholder, so the
    // desktop takes it too, and retypes it where it is when a run wrote it.
    await user.type(cleared, 'Bye')
    await user.tab()
    expect(typed).toHaveBeenCalledTimes(2)
    expect(typed).toHaveBeenLastCalledWith('cleared', 'Bye')
    expect(edits).toEqual([])
  })
})
