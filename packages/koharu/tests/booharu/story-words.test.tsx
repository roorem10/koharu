import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { StoryPanel } from '@/components/editor/StoryPanel'
import { TypeBar } from '@/components/editor/TypeBar'
import { useWordRange } from '@/lib/booharu/words'
import { CANVAS_PRESS } from '@/lib/booharu/wordSelection'
import {
  fontsKey,
  pageKey,
  pagesKey,
  projectKey,
  queryClient,
  type ProjectView,
} from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Edit, type Layer, type Page, type Span } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { comicFonts } from './fonts'

// Booharu: words chosen in the Story panel's fields (S10-l): the word toolbar by the field, its
// close rules, Ctrl+B and Ctrl+I there, and the type bar following the words.

const style = { chars: { size: 24, family: 'Wild Words' } }

/** A block, its translation's spans `spans`. */
function block(id: string, spans: Span[]): Layer {
  return {
    id,
    name: id,
    visible: true,
    locked: false,
    opacity: 1,
    blend: 'normal',
    attributes: {},
    kind: 'block',
    source: { content: [{ insert: 'こんにちは' }] },
    translation: { content: spans, style },
  }
}

/** A text layer, its spans `spans`. */
function textLayer(id: string, spans: Span[]): Layer {
  return {
    id,
    name: id,
    visible: true,
    locked: false,
    opacity: 1,
    blend: 'normal',
    attributes: {},
    kind: 'text',
    content: spans,
    style,
  }
}

function pageOf(...layers: Layer[]): Page {
  return { id: 'page', size: { width: 400, height: 400 }, background: null, layers }
}

/** Puts `page` in the caches, as a change to it does: the canvas's page and Story's. */
function show(page: Page) {
  queryClient.setQueryData(pageKey, page)
  queryClient.setQueryData([...pagesKey, page.id], {
    page,
    story: page.layers.map((layer) => layer.id),
  })
}

/** `page` shown on the canvas, with the type bar when `bar`, and its texts in Story in the
 * sidebar; every command answers null, and the edits sent are kept. */
function story(page: Page, bar = false) {
  const table = commands as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>
  for (const name of Object.keys(table)) vi.spyOn(table, name).mockResolvedValue(null)
  vi.spyOn(commands, 'getFontPreview').mockResolvedValue(new ArrayBuffer(0))
  const edits: Edit[] = []
  vi.spyOn(commands, 'edit').mockImplementation(async (edit) => {
    edits.push(edit)
    return null as never
  })
  const project: ProjectView = {
    name: 'Demo',
    page: page.id,
    pages: [{ id: page.id, size: page.size, background: null }],
    history: { steps: [], current: 0 },
    revisions: {},
  }
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(fontsKey, comicFonts)
  show(page)
  useKoharuStore.setState({
    tool: 'select',
    selectionBounds: { x: 100, y: 100, width: 200, height: 80 },
  })
  const view = render(
    <QueryClientProvider client={queryClient}>
      <main data-testid='canvas'>{bar && <TypeBar />}</main>
      <aside>
        <StoryPanel />
      </aside>
    </QueryClientProvider>,
  )
  return { edits, view }
}

/** Selects characters `start` to `end` of `field`, UTF-16 indices, as a drag across them does. */
function choose(field: HTMLElement, start: number, end: number) {
  ;(field as HTMLTextAreaElement).setSelectionRange(start, end)
  fireEvent.select(field)
}

/** A press on `target`, and the turn a Story field waits for React's handlers to mark it. */
async function press(target: Element) {
  fireEvent.pointerDown(target)
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)))
}

const range = () => useWordRange.getState().range
const toolbar = () => screen.queryByRole('toolbar', { name: 'Word styles' })
const styled = () => vi.mocked(commands.booharuStyleWords).mock.calls

describe('Booharu words chosen in Story', () => {
  it('chooses saved characters in UTF-8 bytes, emoji and CJK included', async () => {
    story(pageOf(block('block', [{ insert: '世界😀 hi' }])))
    const field = await screen.findByPlaceholderText('Translation')

    // The emoji, after two CJK characters of three bytes each, is four bytes.
    choose(field, 2, 4)
    expect(range()).toEqual({ layer: 'block', start: 6, end: 10, text: '世界😀 hi' })
    expect(toolbar()).toBeInTheDocument()
    expect(useWordRange.getState().field).toBe(field)
  })

  it('chooses nothing in a draft not yet saved, and lets chosen words go', async () => {
    story(pageOf(block('block', [{ insert: 'Hello world' }])))
    const field = await screen.findByPlaceholderText('Translation')
    choose(field, 6, 11)
    expect(range()).not.toBeNull()

    fireEvent.change(field, { target: { value: 'Hello world!' } })
    choose(field, 0, 5)
    expect(range()).toBeNull()
    expect(toolbar()).toBeNull()
  })

  it('keeps the words for presses on the field and the toolbar, not for one outside', async () => {
    story(pageOf(block('block', [{ insert: 'Hello world' }])))
    const field = await screen.findByPlaceholderText('Translation')
    choose(field, 6, 11)
    const chosen = range()

    await press(field)
    await press(screen.getByRole('button', { name: 'Bold' }))
    expect(range()).toEqual(chosen)
    expect(toolbar()).toBeInTheDocument()

    await press(screen.getByTestId('canvas'))
    expect(range()).toBeNull()
    expect(toolbar()).toBeNull()
  })

  it('hides the toolbar for a press elsewhere in the sidebar, and keeps the words', async () => {
    story(pageOf(block('block', [{ insert: 'Hello world' }])))
    const field = await screen.findByPlaceholderText('Translation')
    choose(field, 6, 11)
    const chosen = range()

    await press(screen.getByPlaceholderText('Source text'))
    expect(range()).toEqual(chosen)
    expect(toolbar()).toBeNull()

    // A press outside the sidebar still lets them go.
    await press(screen.getByTestId('canvas'))
    expect(range()).toBeNull()
  })

  it("lets the words go for a press on the desktop's canvas, told of by its event", async () => {
    story(pageOf(block('block', [{ insert: 'Hello world' }])))
    const field = await screen.findByPlaceholderText('Translation')
    // The window gives the canvas its presses, so the page hears no pointerdown, only the event.
    const canvasPress = () => act(() => void window.dispatchEvent(new Event(CANVAS_PRESS)))

    choose(field, 6, 11)
    canvasPress()
    expect(range()).toBeNull()
    expect(toolbar()).toBeNull()

    // Words kept by a press elsewhere in the sidebar go too.
    choose(field, 0, 5)
    await press(screen.getByPlaceholderText('Source text'))
    expect(range()).not.toBeNull()
    canvasPress()
    expect(range()).toBeNull()
  })

  it('lets the words go for a caret, Escape, a change of the text and the field going', async () => {
    const { view } = story(pageOf(block('block', [{ insert: 'Hello world' }])))
    const field = await screen.findByPlaceholderText('Translation')

    choose(field, 6, 11)
    choose(field, 3, 3)
    expect(range()).toBeNull()

    choose(field, 0, 5)
    expect(fireEvent.keyDown(field, { key: 'Escape' })).toBe(true)
    expect(range()).toBeNull()
    expect(toolbar()).toBeNull()

    choose(field, 6, 11)
    await act(async () => show(pageOf(block('block', [{ insert: 'Hello there' }]))))
    await waitFor(() => expect(field).toHaveValue('Hello there'))
    expect(range()).toBeNull()

    choose(field, 0, 5)
    expect(range()).toEqual({ layer: 'block', start: 0, end: 5, text: 'Hello there' })
    view.unmount()
    expect(range()).toBeNull()
  })

  it('moves the words, and the toolbar, to another field choosing some', async () => {
    story(
      pageOf(
        block('block', [{ insert: 'Hello world' }]),
        textLayer('text', [{ insert: 'Bye now' }]),
      ),
    )
    const translation = await screen.findByPlaceholderText('Translation')
    choose(translation, 6, 11)
    choose(screen.getByPlaceholderText('Text'), 0, 3)

    expect(range()).toEqual({ layer: 'text', start: 0, end: 3, text: 'Bye now' })
    expect(screen.getAllByRole('toolbar', { name: 'Word styles' })).toHaveLength(1)

    // The first field's press no longer keeps them.
    await press(screen.getByTestId('canvas'))
    expect(range()).toBeNull()
  })

  it("styles the words with each of the toolbar's buttons, the field keeping the focus", async () => {
    const user = userEvent.setup()
    story(
      pageOf(
        block('block', [{ insert: 'Hello ' }, { insert: 'world', attributes: { weight: 700 } }]),
      ),
    )
    const field = await screen.findByPlaceholderText('Translation')
    choose(field, 6, 11)
    const bar = within(toolbar()!)

    // B and I show what the words' characters share, the text's style under their marks.
    expect(bar.getByRole('button', { name: 'Bold' })).toHaveAttribute('aria-pressed', 'true')
    expect(bar.getByRole('button', { name: 'Italic' })).toHaveAttribute('aria-pressed', 'false')
    await user.click(bar.getByRole('button', { name: 'Bold' }))
    await user.click(bar.getByRole('button', { name: 'Italic' }))
    await user.click(bar.getByRole('button', { name: 'Emphasis' }))
    await user.click(bar.getByRole('button', { name: 'Clear' }))

    const words = ['block', 6, 11, 'Hello world'] as const
    expect(styled()).toEqual([
      [...words, { bold: false }],
      [...words, { italic: true }],
      [...words, { emphasis: true }],
      [...words, { clear: true }],
    ])
    expect(field).toHaveFocus()
    expect(range()).not.toBeNull()
  })

  it('reaches the toolbar from the field with Alt+F10, a toolbar the arrows move along', async () => {
    const user = userEvent.setup()
    story(pageOf(block('block', [{ insert: 'Hello world' }])))
    const field = (await screen.findByPlaceholderText('Translation')) as HTMLTextAreaElement
    field.focus()
    choose(field, 6, 11)
    const bar = within(toolbar()!)
    const controls = ['Bold', 'Italic', 'Emphasis', 'Clear'].map((name) =>
      bar.getByRole('button', { name }),
    )
    const [bold, , emphasis, clear] = controls
    const stops = () => controls.map((control) => control.tabIndex)

    await user.keyboard('{Alt>}{F10}{/Alt}')
    expect(bold).toHaveFocus()
    // One tab stop, on the control focused last.
    expect(stops()).toEqual([0, -1, -1, -1])

    // The keys it takes never reach the canvas's shortcuts; a chord does.
    const canvas = vi.fn((event: KeyboardEvent) => event.key)
    window.addEventListener('keydown', canvas)
    await user.keyboard('{ArrowRight}{ArrowRight}')
    expect(emphasis).toHaveFocus()
    expect(stops()).toEqual([-1, -1, 0, -1])
    await user.keyboard('{Enter}{ArrowRight}{ArrowRight}{ArrowLeft}')
    expect(clear).toHaveFocus()
    await user.keyboard('{Home}')
    expect(bold).toHaveFocus()
    await user.keyboard('{End}[Space]{Delete}')
    expect(clear).toHaveFocus()
    expect(canvas).not.toHaveBeenCalled()
    await user.keyboard('{Control>}z{/Control}')
    expect(canvas.mock.results.map(({ value }) => value)).toEqual(['Control', 'z'])
    window.removeEventListener('keydown', canvas)

    const words = ['block', 6, 11, 'Hello world'] as const
    expect(styled()).toEqual([
      [...words, { emphasis: true }],
      [...words, { clear: true }],
    ])

    // Escape takes the focus back to the field, its words still chosen, as does Tab; Alt+F10 there
    // finds the control focused last.
    await user.keyboard('{Escape}')
    expect(field).toHaveFocus()
    expect([field.selectionStart, field.selectionEnd]).toEqual([6, 11])
    expect(range()).toEqual({ layer: 'block', start: 6, end: 11, text: 'Hello world' })
    await user.keyboard('{Alt>}{F10}{/Alt}')
    expect(clear).toHaveFocus()
    await user.keyboard('{Tab}')
    expect(field).toHaveFocus()
  })

  it('does what B and I do for Ctrl+B and Ctrl+I in the field, only with words chosen', async () => {
    story(pageOf(textLayer('text', [{ insert: 'Hello world', attributes: { italic: true } }])))
    const field = await screen.findByPlaceholderText('Text')
    field.focus()

    // Without words, the keys are the field's.
    expect(fireEvent.keyDown(field, { key: 'b', code: 'KeyB', ctrlKey: true })).toBe(true)
    expect(styled()).toEqual([])

    choose(field, 0, 5)
    expect(fireEvent.keyDown(field, { key: 'b', code: 'KeyB', ctrlKey: true })).toBe(false)
    expect(fireEvent.keyDown(field, { key: 'i', code: 'KeyI', metaKey: true })).toBe(false)
    expect(
      fireEvent.keyDown(field, { key: 'B', code: 'KeyB', ctrlKey: true, shiftKey: true }),
    ).toBe(true)
    expect(styled()).toEqual([
      ['text', 0, 5, 'Hello world', { bold: true }],
      ['text', 0, 5, 'Hello world', { italic: false }],
    ])
  })

  it('shows why a call was refused, and lets the words go when their text changed', async () => {
    const user = userEvent.setup()
    story(pageOf(block('block', [{ insert: 'Hello world' }])))
    const shown = vi.spyOn(toast, 'add')
    vi.mocked(commands.booharuStyleWords).mockRejectedValueOnce(
      'The text changed. Select the words again.',
    )
    const field = await screen.findByPlaceholderText('Translation')
    choose(field, 6, 11)

    await user.click(within(toolbar()!).getByRole('button', { name: 'Emphasis' }))

    await waitFor(() => expect(range()).toBeNull())
    expect(shown).toHaveBeenCalledExactlyOnceWith({
      type: 'info',
      title: 'The text changed. Select the words again.',
    })
    expect(toolbar()).toBeNull()
  })

  it('floats above its field, or below it near the top of the window', async () => {
    story(pageOf(block('block', [{ insert: 'Hello world' }])))
    const field = await screen.findByPlaceholderText('Translation')
    const at = (top: number) =>
      vi.spyOn(field, 'getBoundingClientRect').mockReturnValue({
        top,
        bottom: top + 40,
        left: 40,
        right: 240,
        width: 200,
        height: 40,
        x: 40,
        y: top,
        toJSON: () => ({}),
      })

    at(300)
    choose(field, 0, 5)
    expect(toolbar()!.style).toMatchObject({ left: '40px', top: '294px' })
    expect(toolbar()!.style.transform).toBe('translateY(-100%)')

    fireEvent.keyDown(field, { key: 'Escape' })
    at(4)
    choose(field, 6, 11)
    expect(toolbar()!.style).toMatchObject({ left: '40px', top: '50px' })
    expect(toolbar()!.style.transform).toBe('')
  })
})

describe('Booharu type bar on words chosen in Story', () => {
  it("shows the words' values and styles only them, presses on it keeping them", async () => {
    const user = userEvent.setup()
    const border = { stroke: [1, 1, 1, 1] as [number, number, number, number], stroke_width: 2 }
    const { edits } = story(
      pageOf(
        textLayer('text', [
          { insert: 'Hello ' },
          { insert: 'world', attributes: { weight: 700, italic: true, ...border } },
        ]),
      ),
      true,
    )
    const field = await screen.findByPlaceholderText('Text')
    choose(field, 6, 11)
    const bar = within(await screen.findByTestId('type-bar'))

    expect(bar.getByRole('button', { name: 'Bold' })).toHaveAttribute('aria-pressed', 'true')
    expect(bar.getByRole('button', { name: 'Italic' })).toHaveAttribute('aria-pressed', 'true')
    expect(bar.getByRole('combobox', { name: 'Font weight' })).toHaveTextContent('700')

    // B and I as the word toolbar's, the others' values set on the words, and no border a border
    // no wide, as a mark can't unset one.
    await user.click(bar.getByRole('button', { name: 'Bold' }))
    await user.click(bar.getByRole('button', { name: 'Italic' }))
    await user.click(bar.getByRole('combobox', { name: 'Font weight' }))
    await user.click(await screen.findByRole('option', { name: '400' }))
    await user.click(bar.getByRole('button', { name: 'Border' }))
    await user.click(await screen.findByRole('button', { name: 'Border color' }))
    await user.click(await screen.findByRole('button', { name: 'Transparent' }))

    const words = ['text', 6, 11, 'Hello world'] as const
    await waitFor(() =>
      expect(styled()).toEqual([
        [...words, { bold: false }],
        [...words, { italic: false }],
        [...words, { set: { weight: 400 } }],
        [...words, { set: { stroke_width: 0 } }],
      ]),
    )
    expect(edits).toEqual([])
    expect(range()).not.toBeNull()

    // A press outside lets the words go, and the bar shows the whole text again.
    await press(screen.getByTestId('canvas'))
    expect(range()).toBeNull()
    expect(bar.getByRole('button', { name: 'Bold' })).toHaveAttribute('aria-pressed', 'false')
  })

  it("styles the whole text again after a press on the desktop's canvas", async () => {
    story(
      pageOf(
        textLayer('text', [{ insert: 'Hello ' }, { insert: 'world', attributes: { weight: 700 } }]),
      ),
      true,
    )
    choose(await screen.findByPlaceholderText('Text'), 6, 11)
    const bar = within(await screen.findByTestId('type-bar'))
    expect(bar.getByRole('button', { name: 'Bold' })).toHaveAttribute('aria-pressed', 'true')

    act(() => void window.dispatchEvent(new Event(CANVAS_PRESS)))
    expect(range()).toBeNull()
    expect(bar.getByRole('button', { name: 'Bold' })).toHaveAttribute('aria-pressed', 'false')
  })

  it('leaves the second border of words that have none, as typing does', async () => {
    const user = userEvent.setup()
    const border = { stroke: [1, 1, 1, 1] as [number, number, number, number], stroke_width: 2 }
    story(pageOf(textLayer('text', [{ insert: 'Hello world', attributes: border }])), true)
    choose(await screen.findByPlaceholderText('Text'), 0, 5)
    const bar = within(await screen.findByTestId('type-bar'))

    await user.click(bar.getByRole('button', { name: 'Border' }))
    await user.click(await screen.findByRole('button', { name: 'Second border color' }))
    await user.click(await screen.findByRole('button', { name: 'Transparent' }))

    expect(styled()).toEqual([])
    expect(range()).not.toBeNull()
  })
})
