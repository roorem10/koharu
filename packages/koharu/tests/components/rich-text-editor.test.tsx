import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { useCompareStore } from '@/components/qc/compare'
import { defaultTypography } from '@/components/redesign/typography'
import {
  caretEdge,
  emptyRunStyle,
  nearestOffset,
  selectionQuads,
  stepOffset,
  textChange,
  wordAt,
  wordAtPoint,
} from '@/components/rich-text-editor/model'
import { useRichTextEditor } from '@/components/rich-text-editor/useRichTextEditor'
import { pageKey, pagesKey, projectKey, queryClient } from '@/lib/queries'
import { useRichTextSelection } from '@/lib/richTextSelection'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type GlyphBox,
  type Layer,
  type Page,
  type StyleRun,
} from '@koharu/bridge/protocol'

// The controls have their own suites. Keep these tests about the editor's selection and
// command boundary, and retain a portal to exercise the real outside-click behavior.
vi.mock('@/components/controls/FontPicker', () => ({
  FontPicker: ({
    value,
    ariaLabel,
    onChange,
    disabled,
  }: {
    value: string
    ariaLabel: string
    onChange: (s: string) => void
    disabled: boolean
  }) => (
    <select
      aria-label={ariaLabel}
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(e.currentTarget.value)}
    >
      <option value=''>Mixed</option>
      <option value='CCWildWords'>CCWildWords</option>
      <option value='Example'>Example</option>
    </select>
  ),
}))
vi.mock('@/components/controls/ColorWell', async () => {
  const { createPortal } = await import('react-dom')
  return {
    ColorWell: ({
      value,
      label,
      onChange,
      disabled,
    }: {
      value: string
      label: string
      onChange: (s: string) => void
      disabled: boolean
    }) =>
      createPortal(
        <input
          type='color'
          aria-label={label}
          value={value}
          disabled={disabled}
          onChange={(e) => onChange(e.currentTarget.value)}
        />,
        document.body,
      ),
  }
})

let page: Page
let runs: StyleRun[]
const project = {
  name: 'Editor test',
  revision: 1,
  active_page: 'page',
  can_undo: true,
  can_redo: true,
}
const baseLayer: Extract<Layer, { type: 'text' }> = {
  type: 'text',
  id: 'box',
  parent: null,
  geometry: null,
  angle_degrees: null,
  visibility: { visible: true, opacity: 1 },
  content: {
    id: 'content',
    source: null,
    translation: { text: 'A😀 beta', language: 'en' },
    role: null,
    source_region: null,
  },
  typography: { ...defaultTypography, size: 24, auto_fit: false },
  layout: 'paragraph',
  automatic_region: null,
}
const text = () => (page.layers[0] as typeof baseLayer).content.translation!.text
const setText = (value: string) => {
  page = {
    ...page,
    layers: [
      {
        ...(page.layers[0] as typeof baseLayer),
        content: { ...baseLayer.content, translation: { text: value, language: 'en' } },
      },
      ...page.layers.slice(1),
    ],
  }
}
const glyphs = (): GlyphBox[] =>
  Array.from(text(), (_, start) => ({
    start,
    end: start + 1,
    line: 0,
    quad: [
      [20 + start * 10, 60],
      [30 + start * 10, 60],
      [30 + start * 10, 80],
      [20 + start * 10, 80],
    ],
  }))
const canvasDown = vi.fn()

function Harness() {
  const editor = useRichTextEditor()
  return (
    <div onPointerDown={canvasDown}>
      <button
        data-testid='box'
        onDoubleClick={(event) =>
          editor.open('box', { x: event.clientX, y: event.clientY }, event.nativeEvent)
        }
      >
        Box
      </button>
      <button
        data-testid='image'
        onDoubleClick={(event) => editor.open('image', { x: 30, y: 70 }, event.nativeEvent)}
      >
        Image
      </button>
      <button>Outside</button>
      <span>{editor.editing ?? 'closed'}</span>
      {editor.overlay}
    </div>
  )
}
const mount = () =>
  render(
    <QueryClientProvider client={queryClient}>
      <Harness />
    </QueryClientProvider>,
  )
/** Opens the editor (a caret only) and, unless `word` is false, selects "beta" (3..7). */
async function open(x = 55, word = true) {
  fireEvent.doubleClick(screen.getByTestId('box'), { button: 0, clientX: x, clientY: 70 })
  const area = (await screen.findByRole('textbox', {
    name: 'Edit text on the page',
  })) as HTMLTextAreaElement
  await waitFor(() => expect(screen.queryByRole('status')).not.toBeInTheDocument())
  if (!word) return area
  fireEvent.keyDown(area, { key: 'End', ctrlKey: true })
  for (let i = 0; i < 4; i++) fireEvent.keyDown(area, { key: 'ArrowLeft', shiftKey: true })
  await waitFor(() => expect(screen.getByRole('button', { name: 'Bold (Ctrl+B)' })).toBeEnabled())
  return area
}
function change(area: HTMLTextAreaElement, value: string, start = value.length, end = start) {
  fireEvent.change(area, { target: { value, selectionStart: start, selectionEnd: end } })
}

beforeEach(() => {
  class Pointer extends MouseEvent {
    pointerId: number
    constructor(type: string, init: PointerEventInit) {
      super(type, init)
      this.pointerId = init.pointerId ?? 1
    }
  }
  vi.stubGlobal('PointerEvent', Pointer)
  useCompareStore.getState().reset()
  runs = []
  page = {
    id: 'page',
    label: 'Page',
    size: { width: 500, height: 400 },
    layers: [
      structuredClone(baseLayer),
      {
        type: 'image',
        id: 'image',
        parent: null,
        geometry: {
          points: [
            { x: 100, y: 100 },
            { x: 200, y: 100 },
            { x: 200, y: 200 },
          ],
        },
        visibility: { visible: true, opacity: 1 },
        image: 'image',
      },
    ],
    regions: [],
  }
  vi.spyOn(commands, 'getPage').mockImplementation(async () => page)
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  vi.spyOn(commands, 'getPages').mockResolvedValue([])
  vi.spyOn(commands, 'getFonts').mockResolvedValue([])
  vi.mocked(commands.getTextGlyphBoxes).mockImplementation(async () => glyphs())
  vi.mocked(commands.getWordStyles).mockImplementation(async () => runs)
  vi.mocked(commands.replaceTextRange).mockImplementation(async (_layer, start, end, insert) => {
    const chars = Array.from(text())
    setText([...chars.slice(0, start), ...Array.from(insert), ...chars.slice(end)].join(''))
    return null
  })
  vi.mocked(commands.setWordStyle).mockImplementation(async (_layer, start, end, patch) => {
    const style = { ...emptyRunStyle, ...runs[0]?.style }
    for (const [key, value] of Object.entries(patch.set))
      if (value !== null) Object.assign(style, { [key]: value })
    for (const field of patch.clear) style[field] = null
    runs = [{ start, end, style }]
    return null
  })
  vi.spyOn(commands, 'undo').mockResolvedValue(null)
  vi.spyOn(commands, 'redo').mockResolvedValue(null)
  queryClient.setQueryData(pageKey, page)
  queryClient.setQueryData(pagesKey, [])
  queryClient.setQueryData(projectKey, project)
  useKoharuStore.setState({
    canvasPage: 'page',
    selectedLayers: [],
    tool: 'select',
    layerFrames: {
      box: { x: 15, y: 55, width: 85, height: 30, angle_degrees: 0 },
    },
  })
})
afterEach(() => vi.unstubAllGlobals())

describe('on-page editor', () => {
  it('opens from a text double click with only a caret, and keeps the box selected', async () => {
    mount()
    await open(55, false)
    expect(screen.getByTestId('rich-text-frame')).toBeInTheDocument()
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument()
    expect(useRichTextSelection.getState().selection).toBeNull()
    expect(useKoharuStore.getState().selectedLayers).toEqual(['box'])
  })

  it('rejects non-text layers, the original page, other tools and control elements', () => {
    mount()
    fireEvent.doubleClick(screen.getByTestId('image'))
    act(() => useCompareStore.getState().setHeld(true))
    fireEvent.doubleClick(screen.getByTestId('box'))
    act(() => {
      useCompareStore.getState().reset()
      useKoharuStore.getState().setTool('pan')
    })
    fireEvent.doubleClick(screen.getByTestId('box'))
    act(() => useKoharuStore.getState().setTool('select'))
    screen.getByTestId('box').setAttribute('data-canvas-control', '')
    fireEvent.doubleClick(screen.getByTestId('box'))
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument()
    expect(commands.getTextGlyphBoxes).not.toHaveBeenCalled()
  })

  it.each(['Escape', 'Done', 'Outside'])(
    'flushes one Unicode edit when leaving with %s',
    async (method) => {
      mount()
      const area = await open()
      change(area, 'A😀 betaz', 4, 8)
      if (method === 'Escape') fireEvent.keyDown(area, { key: 'Escape' })
      else if (method === 'Done') fireEvent.click(screen.getByRole('button', { name: 'Done' }))
      else fireEvent.pointerDown(screen.getByRole('button', { name: 'Outside' }))
      await waitFor(() => expect(screen.queryByRole('textbox')).not.toBeInTheDocument())
      expect(commands.replaceTextRange).toHaveBeenCalledExactlyOnceWith(
        'box',
        7,
        7,
        'z',
        expect.any(Number),
      )
      expect(useRichTextSelection.getState().selection).toBeNull()
    },
  )

  it('keeps the toolbar hidden for a caret and uses character offsets for emoji selection', async () => {
    mount()
    const area = await open(30, false)
    fireEvent.keyDown(area, { key: 'ArrowRight' })
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument()
    expect(useRichTextSelection.getState().selection).toBeNull()
    fireEvent.keyDown(area, { key: 'ArrowLeft', shiftKey: true })
    expect(useRichTextSelection.getState().selection).toEqual({ layer: 'box', start: 1, end: 2 })
    expect(area.selectionStart).toBe(1)
    expect(area.selectionEnd).toBe(3)
    fireEvent.keyDown(area, { key: 'a', ctrlKey: true })
    expect(useRichTextSelection.getState().selection).toEqual({ layer: 'box', start: 0, end: 7 })
  })

  it('places a caret and drags over glyph quads without starting a canvas gesture', async () => {
    mount()
    await open()
    canvasDown.mockClear()
    const frame = screen.getByTestId('rich-text-frame')
    fireEvent.pointerDown(frame, { button: 0, pointerId: 4, clientX: 30, clientY: 70 })
    fireEvent.pointerMove(frame, { pointerId: 4, clientX: 60, clientY: 70 })
    fireEvent.pointerUp(frame, { pointerId: 4, clientX: 60, clientY: 70 })
    expect(useRichTextSelection.getState().selection).toEqual({ layer: 'box', start: 1, end: 4 })
    expect(canvasDown).not.toHaveBeenCalled()
  })

  it('sets and clears bold/italic, scales pixel sizes, and calls emphasis and clear on the selected words', async () => {
    mount()
    const area = await open()
    fireEvent.click(screen.getByRole('button', { name: 'Bold (Ctrl+B)' }))
    await waitFor(() =>
      expect(commands.setWordStyle).toHaveBeenCalledWith(
        'box',
        3,
        7,
        expect.objectContaining({ set: expect.objectContaining({ font_weight: 700 }) }),
      ),
    )
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Bold (Ctrl+B)' })).toHaveAttribute(
        'aria-pressed',
        'true',
      ),
    )
    fireEvent.keyDown(area, { key: 'b', ctrlKey: true })
    await waitFor(() =>
      expect(commands.setWordStyle).toHaveBeenLastCalledWith(
        'box',
        3,
        7,
        expect.objectContaining({ clear: ['font_weight'] }),
      ),
    )
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Italic (Ctrl+I)' })).toBeEnabled(),
    )
    fireEvent.keyDown(area, { key: 'i', ctrlKey: true })
    await waitFor(() =>
      expect(commands.setWordStyle).toHaveBeenLastCalledWith(
        'box',
        3,
        7,
        expect.objectContaining({ set: expect.objectContaining({ font_style: 'italic' }) }),
      ),
    )
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Italic (Ctrl+I)' })).toHaveAttribute(
        'aria-pressed',
        'true',
      ),
    )
    fireEvent.click(screen.getByRole('button', { name: 'Italic (Ctrl+I)' }))
    await waitFor(() =>
      expect(commands.setWordStyle).toHaveBeenLastCalledWith(
        'box',
        3,
        7,
        expect.objectContaining({ clear: ['font_style'] }),
      ),
    )
    await waitFor(() => expect(screen.getByRole('spinbutton')).toBeEnabled())
    fireEvent.change(screen.getByRole('spinbutton'), { target: { value: '30' } })
    fireEvent.blur(screen.getByRole('spinbutton'))
    await waitFor(() =>
      expect(commands.setWordStyle).toHaveBeenLastCalledWith(
        'box',
        3,
        7,
        expect.objectContaining({ set: expect.objectContaining({ size_scale: 1.25 }) }),
      ),
    )
    await waitFor(() => expect(screen.getByRole('button', { name: 'Emphasis' })).toBeEnabled())
    fireEvent.click(screen.getByRole('button', { name: 'Emphasis' }))
    await waitFor(() => expect(commands.applyEmphasis).toHaveBeenCalledWith('box', 3, 7))
    await waitFor(() => expect(screen.getByRole('button', { name: 'Clear' })).toBeEnabled())
    fireEvent.click(screen.getByRole('button', { name: 'Clear' }))
    await waitFor(() => expect(commands.clearWordStyles).toHaveBeenCalledWith(['box'], [3, 7]))
  })

  it('sets font and colour, and keeps selection while a toolbar portal is used', async () => {
    mount()
    await open()
    fireEvent.change(screen.getByRole('combobox', { name: 'Font' }), {
      target: { value: 'Example' },
    })
    await waitFor(() =>
      expect(commands.setWordStyle).toHaveBeenCalledWith(
        'box',
        3,
        7,
        expect.objectContaining({ set: expect.objectContaining({ font: 'Example' }) }),
      ),
    )
    await waitFor(() => expect(screen.getByLabelText('Word colour')).toBeEnabled())
    fireEvent.pointerDown(screen.getByLabelText('Word colour'))
    fireEvent.change(screen.getByLabelText('Word colour'), { target: { value: '#123456' } })
    await waitFor(() =>
      expect(commands.setWordStyle).toHaveBeenCalledWith(
        'box',
        3,
        7,
        expect.objectContaining({ set: expect.objectContaining({ color: [18, 52, 86, 255] }) }),
      ),
    )
    expect(screen.getByRole('textbox')).toBeInTheDocument()
    expect(useRichTextSelection.getState().selection).toEqual({ layer: 'box', start: 3, end: 7 })
  })

  it('reports mixed values for runs and inherited gaps and disables unknown auto-fit pixels', async () => {
    runs = [
      {
        start: 4,
        end: 5,
        style: {
          ...emptyRunStyle,
          font_weight: 700,
          size_scale: 1.5,
          font: 'Example',
          color: [255, 0, 0, 255],
        },
      },
    ]
    page.layers[0] = { ...baseLayer, typography: { ...defaultTypography, auto_fit: true } }
    queryClient.setQueryData(pageKey, page)
    mount()
    await open()
    expect(screen.getByRole('button', { name: 'Bold (Ctrl+B)' })).toHaveAttribute(
      'aria-pressed',
      'mixed',
    )
    expect(screen.getByRole('combobox')).toHaveValue('')
    expect(screen.getByRole('spinbutton')).toBeDisabled()
    expect(screen.getByLabelText('Word colour: Mixed')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Bigger' }))
    await waitFor(() =>
      expect(commands.setWordStyle).toHaveBeenLastCalledWith(
        'box',
        3,
        7,
        expect.objectContaining({ set: expect.objectContaining({ size_scale: 1.05 }) }),
      ),
    )
    await waitFor(() => expect(screen.getByRole('button', { name: 'Smaller' })).toBeEnabled())
    fireEvent.click(screen.getByRole('button', { name: 'Smaller' }))
    await waitFor(() =>
      expect(commands.setWordStyle).toHaveBeenLastCalledWith(
        'box',
        3,
        7,
        expect.objectContaining({ set: expect.objectContaining({ size_scale: 1 }) }),
      ),
    )
  })

  it('debounces a typing chunk and fetches fresh glyphs after the commit and canvas updates', async () => {
    mount()
    const area = await open()
    change(area, 'A😀 beta!')
    change(area, 'A😀 beta!!')
    await waitFor(() =>
      expect(commands.replaceTextRange).toHaveBeenCalledExactlyOnceWith(
        'box',
        7,
        7,
        '!!',
        expect.any(Number),
      ),
    )
    const reads = vi.mocked(commands.getTextGlyphBoxes).mock.calls.length
    act(() => useKoharuStore.setState({ canvasGeneration: 4 }))
    await waitFor(() =>
      expect(vi.mocked(commands.getTextGlyphBoxes).mock.calls.length).toBeGreaterThan(reads),
    )
  })

  it('keeps composition local until it ends, pastes plain text, and deletes a whole non-BMP character', async () => {
    mount()
    const area = await open()
    fireEvent.compositionStart(area)
    change(area, 'A😀 日本')
    await new Promise((resolve) => setTimeout(resolve, 450))
    expect(commands.replaceTextRange).not.toHaveBeenCalled()
    fireEvent.compositionEnd(area, { data: '日本' })
    fireEvent.keyDown(area, { key: 'Home', ctrlKey: true })
    fireEvent.keyDown(area, { key: 'ArrowRight' })
    fireEvent.keyDown(area, { key: 'Delete' })
    expect(area).toHaveValue('A 日本')
    fireEvent.paste(area, {
      clipboardData: { getData: (kind: string) => (kind === 'text/plain' ? '🎉' : '<b>wrong</b>') },
    })
    fireEvent.keyDown(area, { key: 'Escape' })
    await waitFor(() =>
      expect(commands.replaceTextRange).toHaveBeenCalledExactlyOnceWith(
        'box',
        1,
        7,
        '🎉 日本',
        expect.any(Number),
      ),
    )
  })

  it.each(['z', 'y'])('flushes typing before app undo/redo (Ctrl+%s)', async (key) => {
    mount()
    const area = await open()
    change(area, 'A😀 beta!')
    fireEvent.keyDown(area, { key, ctrlKey: true })
    const command = key === 'z' ? commands.undo : commands.redo
    await waitFor(() => expect(command).toHaveBeenCalledOnce())
    expect(commands.replaceTextRange).toHaveBeenCalledExactlyOnceWith(
      'box',
      7,
      7,
      '!',
      expect.any(Number),
    )
    expect(vi.mocked(commands.replaceTextRange).mock.invocationCallOrder[0]).toBeLessThan(
      vi.mocked(command).mock.invocationCallOrder[0],
    )
  })

  it.each(['tool', 'page', 'original'])(
    'flushes when changing %s and clears the shared selection',
    async (what) => {
      mount()
      const area = await open()
      change(area, 'A😀 beta!')
      act(() => {
        if (what === 'tool') useKoharuStore.getState().setTool('pan')
        if (what === 'page') queryClient.setQueryData(pageKey, { ...page, id: 'other' })
        if (what === 'original') useCompareStore.getState().setHeld(true)
      })
      await waitFor(() => expect(screen.queryByRole('textbox')).not.toBeInTheDocument())
      expect(commands.replaceTextRange).toHaveBeenCalledExactlyOnceWith(
        'box',
        7,
        7,
        '!',
        expect.any(Number),
      )
      expect(useRichTextSelection.getState().selection).toBeNull()
    },
  )

  it('preserves unsaved text on failure and retries without losing the draft', async () => {
    vi.mocked(commands.replaceTextRange).mockRejectedValueOnce(new Error('disk full'))
    mount()
    const area = await open()
    change(area, 'A😀 beta!')
    fireEvent.keyDown(area, { key: 'Escape' })
    expect(await screen.findByRole('button', { name: 'Retry and finish' })).toBeInTheDocument()
    expect(area).toHaveValue('A😀 beta!')
    fireEvent.click(screen.getByRole('button', { name: 'Retry and finish' }))
    await waitFor(() => expect(screen.queryByRole('textbox')).not.toBeInTheDocument())
    expect(text()).toBe('A😀 beta!')
  })

  it('saves edits made while an earlier typing chunk is still being committed', async () => {
    const replace = vi.mocked(commands.replaceTextRange).getMockImplementation()!
    let release = () => {}
    vi.mocked(commands.replaceTextRange).mockImplementationOnce(async (...args) => {
      await new Promise<void>((resolve) => {
        release = resolve
      })
      return replace(...args)
    })
    mount()
    const area = await open()
    change(area, 'A😀 beta!')
    await waitFor(() => expect(commands.replaceTextRange).toHaveBeenCalledOnce())
    change(area, 'A😀 beta!!')
    fireEvent.keyDown(area, { key: 'Escape' })
    await act(async () => release())
    await waitFor(() => expect(screen.queryByRole('textbox')).not.toBeInTheDocument())
    expect(commands.replaceTextRange).toHaveBeenNthCalledWith(
      1,
      'box',
      7,
      7,
      '!',
      expect.any(Number),
    )
    expect(commands.replaceTextRange).toHaveBeenNthCalledWith(
      2,
      'box',
      8,
      8,
      '!',
      expect.any(Number),
    )
    const [first, second] = vi.mocked(commands.replaceTextRange).mock.calls
    // Chunks typed in one editing session become one undo step.
    expect(second![4]).toBe(first![4])
    expect(text()).toBe('A😀 beta!!')
  })
})

describe('glyph selection geometry', () => {
  it('selects the final letter of a word by its glyph, and the next word at a shared edge', () => {
    const boxes = glyphs()
    expect(wordAtPoint(text(), boxes, { x: 89, y: 70 }, false)).toEqual([3, 7])
    expect(wordAtPoint(text(), boxes, { x: 30, y: 70 }, false)).toEqual([1, 2])
  })
  it('interpolates a ligature caret and selection after rotation and skew', () => {
    const glyph: GlyphBox = {
      start: 1,
      end: 3,
      line: 0,
      quad: [
        [10, 10],
        [20, 30],
        [10, 35],
        [0, 15],
      ],
    }
    expect(caretEdge([glyph], 2, false)).toEqual([
      [15, 20],
      [5, 25],
    ])
    expect(selectionQuads([glyph], 1, 2, false)).toEqual([
      [
        [10, 10],
        [15, 20],
        [5, 25],
        [0, 15],
      ],
    ])
    expect(nearestOffset([glyph], { x: 10, y: 22 }, false)).toBe(2)
  })
  it('uses the top and bottom edges for vertical characters', () => {
    const glyph: GlyphBox = {
      start: 0,
      end: 1,
      line: 0,
      quad: [
        [10, 10],
        [30, 10],
        [30, 40],
        [10, 40],
      ],
    }
    expect(caretEdge([glyph], 1, true)).toEqual([
      [10, 40],
      [30, 40],
    ])
    expect(nearestOffset([glyph], { x: 20, y: 38 }, true)).toBe(1)
  })
  it('segments Japanese words, traverses graphemes, and computes a minimal edit in char offsets', () => {
    expect(wordAt('日本語です', 1)).toEqual([0, 3])
    expect(stepOffset('A😀e\u0301Z', 4, -1)).toBe(2)
    expect(textChange('A😀 beta', 'A😺 beta')).toEqual({ start: 1, end: 2, insert: '😺' })
  })
})
