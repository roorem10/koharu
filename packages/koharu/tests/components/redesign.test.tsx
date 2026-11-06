import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { useCanvasContextMenu } from '@/components/editor/CanvasContextMenu'
import { Inspector } from '@/components/editor/Inspector'
import { useCompareStore } from '@/components/qc/compare'
import { useRedesignStore } from '@/components/redesign/state'
import { defaultFont, defaultTypography } from '@/components/redesign/typography'
import {
  emptyRunStyle,
  selectedTypography,
  wordStylePatch,
} from '@/components/redesign/useWordTypography'
import { fontsKey, pageKey, projectKey, queryClient } from '@/lib/queries'
import { useRichTextSelection } from '@/lib/richTextSelection'
import { useKoharuStore } from '@/lib/store'
import translations from '@/public/locales/en-US/translation.json'
import { commands, type Layer, type Page, type Typography } from '@koharu/bridge/protocol'

const issues = vi.hoisted(() => new Map<string, number>())
vi.mock('@/components/qc/useBoxIssues', () => ({ useBoxIssues: () => issues }))
vi.mock('@/components/qc/BoxQcSection', () => ({
  BoxQcSection: ({ layer }: { layer: string }) => <span data-testid='qc-component'>{layer}</span>,
}))
vi.mock('@/components/ocr-box/BoxRedoSection', () => ({
  BoxRedoSection: ({ layers }: { layers: string[] }) => (
    <span data-testid='redo-component'>{layers.join(',')}</span>
  ),
}))
vi.mock('@/components/warp/TextWarpControls', () => ({
  TextWarpControls: ({ layers }: { layers: string[] }) => (
    <span data-testid='warp-component'>{layers.join(',')}</span>
  ),
}))

function text(
  id: string,
  x: number,
  typography: Partial<Typography> = {},
): Extract<Layer, { type: 'text' }> {
  return {
    type: 'text',
    id,
    parent: 'page',
    angle_degrees: 0,
    layout: 'paragraph',
    automatic_region: null,
    visibility: { visible: true, opacity: 1 },
    geometry: {
      points: [
        { x, y: 10 },
        { x: x + 50, y: 10 },
        { x: x + 50, y: 60 },
        { x, y: 60 },
      ],
    },
    content: {
      id: `${id}-text`,
      source: { text: `原文${id}`, language: 'ja' },
      translation: { text: id, language: 'en' },
      role: null,
      source_region: null,
    },
    typography: { ...defaultTypography, size: 18, auto_fit: false, ...typography },
  }
}

function install(
  layers = [text('one', 10), text('two', 100)],
  selected = ['one'],
  regions: Page['regions'] = [],
) {
  issues.clear()
  useCompareStore.getState().reset()
  let page: Page = {
    id: 'page',
    label: 'Page',
    size: { width: 300, height: 200 },
    layers,
    regions,
  }
  let project = {
    name: 'chapter',
    revision: 1,
    active_page: 'page',
    can_undo: false,
    can_redo: false,
  }
  queryClient.setQueryData(pageKey, page)
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(fontsKey, [
    {
      ...defaultFont,
      faces: [400, 700].flatMap((weight) =>
        (['normal', 'italic'] as const).map((style) => ({
          postscript_name: `${style}${weight}`,
          weight,
          weight_range: null,
          style,
        })),
      ),
    },
  ])
  useKoharuStore.setState({ selectedLayers: selected, canvasPage: 'page', canvasRevision: 1 })
  vi.spyOn(commands, 'getPage').mockImplementation(async () => page)
  vi.spyOn(commands, 'getProject').mockImplementation(async () => project)
  const update = vi.spyOn(commands, 'setTypography').mockImplementation(async (updates) => {
    page = {
      ...page,
      layers: page.layers.map((layer) => {
        const typography = updates.find((update) => update.layer === layer.id)?.typography
        return layer.type === 'text' && typography ? { ...layer, typography } : layer
      }),
    }
    project = { ...project, revision: project.revision + 1 }
    return null
  })
  return { update }
}

function MenuHarness() {
  const context = useCanvasContextMenu()
  return (
    <>
      <div
        data-testid='surface'
        aria-busy={false}
        onContextMenu={(event) =>
          context.open(event.nativeEvent, { x: event.clientX, y: event.clientY })
        }
      />
      {context.menu}
    </>
  )
}

function editor(menu = false) {
  return render(
    <QueryClientProvider client={queryClient}>
      <Inspector />
      {menu && <MenuHarness />}
    </QueryClientProvider>,
  )
}

describe('text panel redesign', () => {
  it('applies a word font and colour without copying the box style into the range', async () => {
    const { update } = install()
    queryClient.setQueryData(fontsKey, [defaultFont, { ...defaultFont, name: 'Other' }])
    vi.spyOn(commands, 'getFontFavorites').mockResolvedValue([])
    vi.spyOn(commands, 'getFontPreview').mockResolvedValue([])
    useRichTextSelection.getState().setSelection({ layer: 'one', start: 0, end: 2 })
    editor()
    const picker = screen.getByRole('button', { name: 'Font' })
    await waitFor(() => expect(picker).toBeEnabled())
    await userEvent.click(picker)
    await userEvent.click(await screen.findByRole('option', { name: /^Other/ }))
    const set = vi.mocked(commands.setWordStyle)
    expect(set).toHaveBeenLastCalledWith('one', 0, 2, {
      set: { ...emptyRunStyle, font: 'Other' },
      clear: [],
    })
    const color = screen.getByRole('button', { name: 'Text color' })
    await waitFor(() => expect(color).toBeEnabled())
    await userEvent.click(color)
    fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
      target: { value: '#123456' },
    })
    await waitFor(() => expect(set).toHaveBeenCalledTimes(2))
    expect(set).toHaveBeenLastCalledWith('one', 0, 2, {
      set: { ...emptyRunStyle, color: [18, 52, 86, 255] },
      clear: [],
    })
    expect(update).not.toHaveBeenCalled()
  })

  it('sets primary and second outline colours on the selected words', async () => {
    const { update } = install([text('one', 10, { stroke_width: 2, outer_stroke_width: 3 })])
    useRichTextSelection.getState().setSelection({ layer: 'one', start: 0, end: 2 })
    editor()
    await userEvent.click(screen.getByRole('tab', { name: 'Outline' }))
    const set = vi.mocked(commands.setWordStyle)
    for (const [label, style] of [
      ['Border color', { stroke_color: [18, 52, 86, 255] }],
      ['Second border color', { outer_stroke_color: [18, 52, 86, 255] }],
    ] as const) {
      const button = screen.getByRole('button', { name: label })
      await waitFor(() => expect(button).toBeEnabled())
      await userEvent.click(button)
      fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
        target: { value: '#123456' },
      })
      await waitFor(() =>
        expect(set).toHaveBeenLastCalledWith('one', 0, 2, {
          set: { ...emptyRunStyle, ...style },
          clear: [],
        }),
      )
      await userEvent.keyboard('{Escape}')
    }
    expect(update).not.toHaveBeenCalled()
  })

  it('keeps word gradients editable and refreshes mixed styles after a project revision', async () => {
    const gradient = {
      kind: 'Linear' as const,
      angle_degrees: 45,
      stops: [
        { offset: 0, color: [255, 0, 0, 255] as [number, number, number, number] },
        { offset: 1, color: [0, 0, 255, 255] as [number, number, number, number] },
      ],
    }
    install()
    const get = vi
      .mocked(commands.getWordStyles)
      .mockResolvedValue([
        { start: 0, end: 3, style: { ...emptyRunStyle, fill_gradient: gradient } },
      ])
    useRichTextSelection.getState().setSelection({ layer: 'one', start: 0, end: 3 })
    editor()
    await userEvent.click(screen.getByRole('tab', { name: 'Fill' }))
    const angle = await screen.findByRole('textbox', { name: 'Gradient angle' })
    await waitFor(() => expect(angle).toBeEnabled())
    fireEvent.change(angle, { target: { value: '90' } })
    fireEvent.blur(angle)
    await waitFor(() =>
      expect(commands.setWordStyle).toHaveBeenCalledWith('one', 0, 3, {
        set: { ...emptyRunStyle, fill_gradient: { ...gradient, angle_degrees: 90 } },
        clear: [],
      }),
    )
    get.mockResolvedValue([
      { start: 0, end: 1, style: { ...emptyRunStyle, font: 'Other', font_weight: 700 } },
    ])
    act(() =>
      queryClient.setQueryData(projectKey, { name: 'chapter', revision: 2, active_page: 'page' }),
    )
    await userEvent.click(screen.getByRole('tab', { name: 'Text' }))
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Font' })).toHaveTextContent('Mixed'),
    )
    expect(screen.getByRole('button', { name: 'Bold' })).toHaveAttribute('data-mixed', 'true')
    expect(get).toHaveBeenCalledWith('one')
  })

  it('does not use a stale word range for a different or multiple selected boxes', async () => {
    const { update } = install()
    useRichTextSelection.getState().setSelection({ layer: 'one', start: 0, end: 1 })
    editor()
    await waitFor(() => expect(screen.getByRole('button', { name: 'Bold' })).toBeEnabled())
    act(() => useKoharuStore.setState({ selectedLayers: ['one', 'two'] }))
    expect(screen.queryByRole('button', { name: 'Whole box' })).not.toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Bold' }))
    await waitFor(() => expect(update).toHaveBeenCalledTimes(1))
    expect(commands.setWordStyle).not.toHaveBeenCalled()
    act(() => useKoharuStore.setState({ selectedLayers: ['two'] }))
    expect(screen.queryByRole('button', { name: 'Whole box' })).not.toBeInTheDocument()
  })

  it('queues the last colour from an open picker while an earlier word edit is saving', async () => {
    install()
    let finish: () => void = () => undefined
    const set = vi.mocked(commands.setWordStyle).mockImplementationOnce(
      () =>
        new Promise<null>((resolve) => {
          finish = () => resolve(null)
        }),
    )
    useRichTextSelection.getState().setSelection({ layer: 'one', start: 0, end: 3 })
    editor()
    const color = screen.getByRole('button', { name: 'Text color' })
    await waitFor(() => expect(color).toBeEnabled())
    await userEvent.click(color)
    const hex = screen.getByRole('textbox', { name: 'Hex color code' })
    fireEvent.change(hex, { target: { value: '#123456' } })
    await waitFor(() => expect(set).toHaveBeenCalledTimes(1))
    fireEvent.change(hex, { target: { value: '#ABCDEF' } })
    expect(set).toHaveBeenCalledTimes(1)
    act(() => finish())
    await waitFor(() => expect(set).toHaveBeenCalledTimes(2))
    expect(set).toHaveBeenLastCalledWith('one', 0, 3, {
      set: { ...emptyRunStyle, color: [171, 205, 239, 255] },
      clear: [],
    })
  })

  it('keeps font-only word styles out of FX and respects inherited box gradients', async () => {
    const gradient = {
      kind: 'Linear' as const,
      angle_degrees: 0,
      stops: [
        { offset: 0, color: [0, 0, 0, 255] as [number, number, number, number] },
        { offset: 1, color: [255, 255, 255, 255] as [number, number, number, number] },
      ],
    }
    install([{ ...text('one', 10, { fill_gradient: gradient }), word_styles: { effects: false } }])
    useRichTextSelection.getState().setSelection({ layer: 'one', start: 0, end: 3 })
    editor()
    expect(await screen.findByText('Floating · styled words')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Text effects for one' })).not.toBeInTheDocument()
    await userEvent.click(screen.getByRole('tab', { name: 'Fill' }))
    const mode = screen.getByRole('combobox', { name: 'Fill' })
    await waitFor(() => expect(mode).toBeEnabled())
    await userEvent.click(mode)
    expect(await screen.findByRole('option', { name: 'Color' })).toHaveAttribute(
      'aria-disabled',
      'true',
    )
  })

  it('scopes the panel to Unicode character ranges and Whole box restores the box controls', async () => {
    const layer = text('one', 10)
    layer.content.translation!.text =
      'A😀bold text that is long enough to need truncation in the scope line'
    install([layer])
    editor()
    expect(screen.queryByRole('button', { name: 'Whole box' })).not.toBeInTheDocument()
    act(() => useRichTextSelection.getState().setSelection({ layer: 'one', start: 1, end: 6 }))
    expect(screen.getByText('Applies to "😀bold"')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Align left' })).toBeDisabled()
    expect(screen.getByRole('combobox', { name: 'Text direction' })).toBeDisabled()
    expect(screen.getByRole('textbox', { name: 'Line spacing' })).toBeDisabled()
    expect(screen.getByRole('textbox', { name: 'Letter spacing' })).toBeDisabled()
    expect(screen.getByText('These controls apply to the whole box.')).toBeInTheDocument()
    act(() => useRichTextSelection.getState().setSelection({ layer: 'one', start: 0, end: 50 }))
    expect(
      screen.getByTitle([...layer.content.translation!.text].slice(0, 50).join('')),
    ).toHaveClass('truncate')
    await userEvent.click(screen.getByRole('button', { name: 'Whole box' }))
    expect(useRichTextSelection.getState().selection).toBeNull()
    expect(screen.getByRole('button', { name: 'Align left' })).toBeEnabled()
    act(() => useRichTextSelection.getState().setSelection({ layer: 'absent', start: 0, end: 1 }))
    expect(screen.queryByRole('button', { name: 'Whole box' })).not.toBeInTheDocument()
  })

  it('sets only the chosen word fields, including mixed values and pixel size scales', async () => {
    const { update } = install()
    vi.mocked(commands.getWordStyles).mockResolvedValue([
      { start: 0, end: 1, style: { ...emptyRunStyle, font_weight: 700, size_scale: 1.5 } },
    ])
    useRichTextSelection.getState().setSelection({ layer: 'one', start: 0, end: 3 })
    editor()
    const bold = screen.getByRole('button', { name: 'Bold' })
    await waitFor(() => expect(bold).toBeEnabled())
    expect(screen.getByTestId('type-size')).toHaveAttribute('placeholder', 'Mixed')
    expect(bold).toHaveAttribute('data-mixed', 'true')
    expect(screen.getByRole('combobox', { name: 'Font weight' })).toHaveTextContent('Mixed')
    const set = vi.mocked(commands.setWordStyle)
    await userEvent.click(bold)
    await waitFor(() => expect(set).toHaveBeenCalledTimes(1))
    expect(set).toHaveBeenLastCalledWith('one', 0, 3, {
      set: { ...emptyRunStyle, font_weight: 700 },
      clear: [],
    })
    const italic = screen.getByRole('button', { name: 'Italic' })
    await waitFor(() => expect(italic).toBeEnabled())
    await userEvent.click(italic)
    expect(set).toHaveBeenLastCalledWith('one', 0, 3, {
      set: { ...emptyRunStyle, font_style: 'italic' },
      clear: [],
    })
    const size = screen.getByTestId('type-size')
    await waitFor(() => expect(size).toBeEnabled())
    fireEvent.change(size, { target: { value: '27' } })
    fireEvent.blur(size)
    await waitFor(() => expect(set).toHaveBeenCalledTimes(3))
    expect(set).toHaveBeenLastCalledWith('one', 0, 3, {
      set: { ...emptyRunStyle, size_scale: 1.5 },
      clear: [],
    })
    await waitFor(() => expect(screen.getByRole('combobox', { name: 'Font weight' })).toBeEnabled())
    await userEvent.click(screen.getByRole('combobox', { name: 'Font weight' }))
    await userEvent.click(await screen.findByRole('option', { name: '400' }))
    expect(set).toHaveBeenLastCalledWith('one', 0, 3, {
      set: { ...emptyRunStyle, font_weight: 400 },
      clear: [],
    })
    expect(update).not.toHaveBeenCalled()
  })

  it('routes both outlines and fill to words while outline geometry stays disabled', async () => {
    const { update } = install([text('one', 10, { stroke_width: 2 })])
    useRichTextSelection.getState().setSelection({ layer: 'one', start: 1, end: 3 })
    editor()
    await userEvent.click(screen.getByRole('tab', { name: 'Outline' }))
    const width = screen.getByRole('textbox', { name: 'Border width' })
    await waitFor(() => expect(width).toBeEnabled())
    fireEvent.change(width, { target: { value: '4' } })
    fireEvent.blur(width)
    const set = vi.mocked(commands.setWordStyle)
    await waitFor(() => expect(set).toHaveBeenCalledTimes(1))
    expect(set).toHaveBeenLastCalledWith('one', 1, 3, {
      set: { ...emptyRunStyle, stroke_width: 4 },
      clear: [],
    })
    expect(screen.getByRole('combobox', { name: 'Outline method' })).toBeDisabled()
    expect(screen.getByRole('combobox', { name: 'Position' })).toBeDisabled()
    expect(screen.getByRole('combobox', { name: 'Corners' })).toBeDisabled()
    const outer = screen.getByRole('textbox', { name: 'Second border width' })
    await waitFor(() => expect(outer).toBeEnabled())
    fireEvent.change(outer, { target: { value: '6' } })
    fireEvent.blur(outer)
    await waitFor(() => expect(set).toHaveBeenCalledTimes(2))
    expect(set).toHaveBeenLastCalledWith('one', 1, 3, {
      set: { ...emptyRunStyle, outer_stroke_width: 6 },
      clear: [],
    })
    await userEvent.click(screen.getByRole('tab', { name: 'Fill' }))
    const color = screen.getByRole('button', { name: 'Text color' })
    await waitFor(() => expect(color).toBeEnabled())
    await userEvent.click(color)
    fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
      target: { value: '#123456' },
    })
    await waitFor(() => expect(set).toHaveBeenCalledTimes(3))
    expect(set).toHaveBeenLastCalledWith('one', 1, 3, {
      set: { ...emptyRunStyle, color: [18, 52, 86, 255] },
      clear: [],
    })
    await userEvent.keyboard('{Escape}')
    const mode = screen.getByRole('combobox', { name: 'Fill' })
    await waitFor(() => expect(mode).toBeEnabled())
    await userEvent.click(mode)
    await userEvent.click(await screen.findByRole('option', { name: 'Gradient' }))
    expect(set).toHaveBeenLastCalledWith('one', 1, 3, {
      set: {
        ...emptyRunStyle,
        fill_gradient: {
          kind: 'Linear',
          angle_degrees: 0,
          stops: [
            { offset: 0, color: [0, 0, 0, 255] },
            { offset: 1, color: [255, 255, 255, 255] },
          ],
        },
      },
      clear: [],
    })
    expect(update).not.toHaveBeenCalled()
  })

  it('clears all words only for a single styled box without a word selection and lights word FX', async () => {
    const layer = { ...text('one', 10), word_styles: { effects: true } }
    const { update } = install([layer])
    editor()
    expect(await screen.findByText('Floating · styled words')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Text effects for one' })).toBeInTheDocument()
    const clear = screen.getByRole('button', { name: 'Clear all word styles in this box' })
    await userEvent.click(clear)
    expect(commands.clearWordStyles).toHaveBeenCalledExactlyOnceWith(['one'], null)
    expect(update).not.toHaveBeenCalled()
    act(() => useRichTextSelection.getState().setSelection({ layer: 'one', start: 0, end: 1 }))
    expect(
      screen.queryByRole('button', { name: 'Clear all word styles in this box' }),
    ).not.toBeInTheDocument()
    act(() => {
      useRichTextSelection.getState().setSelection(null)
      queryClient.setQueryData(pageKey, {
        ...queryClient.getQueryData<Page>(pageKey)!,
        layers: [{ ...layer, word_styles: undefined }],
      })
    })
    expect(
      screen.queryByRole('button', { name: 'Clear all word styles in this box' }),
    ).not.toBeInTheDocument()
  })

  it('includes inherited gaps and compares effective values without flattening untouched fields', () => {
    const selection = { layer: 'one', start: 0, end: 4 }
    const base = {
      ...defaultTypography,
      size: 20,
      color: [10, 20, 30, 255] as [number, number, number, number],
    }
    const values = selectedTypography(
      base,
      [
        {
          start: 1,
          end: 3,
          style: { ...emptyRunStyle, size_scale: 2, font: 'Other', color: [255, 0, 0, 255] },
        },
      ],
      selection,
    )
    expect(values.map((value) => value.size)).toEqual([20, 40, 20])
    expect(values.map((value) => value.preferred_font)).toEqual([
      'CCWildWords',
      'Other',
      'CCWildWords',
    ])
    expect(wordStylePatch({ ...values[0], stroke_width: 3 }, ['stroke_width'], 20)).toEqual({
      set: { ...emptyRunStyle, stroke_width: 3 },
      clear: [],
    })
    expect(wordStylePatch({ ...values[0], fill_gradient: null }, ['fill_gradient'], 20)).toEqual({
      set: emptyRunStyle,
      clear: ['fill_gradient'],
    })
  })

  it('shows only the chosen tab and remembers it across remounts', async () => {
    install()
    const view = editor()
    expect(screen.getByTestId('type-size')).toBeInTheDocument()
    expect(screen.queryByRole('textbox', { name: 'Border width' })).not.toBeInTheDocument()
    await userEvent.click(screen.getByRole('tab', { name: 'Outline' }))
    expect(screen.getByRole('textbox', { name: 'Border width' })).toBeInTheDocument()
    expect(screen.queryByTestId('type-size')).not.toBeInTheDocument()
    expect(screen.queryByTestId('lettering-category')).not.toBeInTheDocument()
    view.unmount()
    editor()
    expect(screen.getByRole('tab', { name: 'Outline' })).toHaveAttribute('aria-selected', 'true')
    await userEvent.click(screen.getByRole('tab', { name: 'Fill' }))
    expect(screen.getByRole('combobox', { name: 'Fill' })).toBeInTheDocument()
  })

  it('shows a mixed size and commits one edit to both boxes', async () => {
    const { update } = install(
      [text('one', 10, { size: 14 }), text('two', 100, { size: 30 })],
      ['one', 'two'],
    )
    editor()
    const input = screen.getByTestId('type-size')
    expect(input).toHaveValue('')
    expect(input).toHaveAttribute('placeholder', 'Mixed')
    fireEvent.change(input, { target: { value: '20' } })
    fireEvent.blur(input)
    await waitFor(() => expect(update).toHaveBeenCalledTimes(1))
    expect(update.mock.calls[0][0]).toEqual(
      ['one', 'two'].map((layer) => ({
        layer,
        typography: expect.objectContaining({ size: 20, auto_fit: false }),
      })),
    )
  })

  it('applies bold and italic to mixed selections using available font weights', async () => {
    const { update } = install(
      [text('one', 10), text('two', 100, { font_weight: 700, font_style: 'italic' })],
      ['one', 'two'],
    )
    editor()
    const bold = screen.getByRole('button', { name: 'Bold' })
    const italic = screen.getByRole('button', { name: 'Italic' })
    expect(bold).toHaveAttribute('aria-pressed', 'false')
    expect(bold).toHaveAttribute('title', expect.stringContaining('mixed'))
    expect(italic).toHaveAttribute('aria-pressed', 'false')
    await userEvent.click(bold)
    await waitFor(() => expect(update).toHaveBeenCalledTimes(1))
    expect(update.mock.calls[0][0].map((item) => item.typography.font_weight)).toEqual([700, 700])
    await userEvent.click(italic)
    await waitFor(() => expect(update).toHaveBeenCalledTimes(2))
    expect(update.mock.calls[1][0].map((item) => item.typography.font_style)).toEqual([
      'italic',
      'italic',
    ])
  })

  it('settles a size wheel burst as one typography command for the whole selection', async () => {
    const { update } = install(undefined, ['one', 'two'])
    editor()
    const input = screen.getByTestId('type-size')
    act(() => input.focus())
    fireEvent.wheel(input, { deltaY: -100 })
    fireEvent.wheel(input, { deltaY: -100 })
    fireEvent.wheel(input, { deltaY: -100, shiftKey: true })
    expect(input).toHaveValue('21.5')
    expect(update).not.toHaveBeenCalled()
    await waitFor(() => expect(update).toHaveBeenCalledTimes(1))
    expect(update.mock.calls[0][0].map((item) => item.typography.size)).toEqual([21.5, 21.5])
  })

  it('keeps bold and italic usable on a family with only a regular face', async () => {
    const { update } = install(undefined, ['one', 'two'])
    queryClient.setQueryData(fontsKey, [defaultFont])
    editor()
    const italic = screen.getByRole('button', { name: 'Italic' })
    expect(italic).toBeEnabled()
    await userEvent.click(screen.getByRole('button', { name: 'Bold' }))
    await waitFor(() => expect(update).toHaveBeenCalledTimes(1))
    expect(update.mock.calls[0][0].map((item) => item.typography.font_weight)).toEqual([700, 700])
    await userEvent.click(italic)
    await waitFor(() => expect(update).toHaveBeenCalledTimes(2))
    expect(update.mock.calls[1][0].map((item) => item.typography.font_style)).toEqual([
      'italic',
      'italic',
    ])
  })

  it('queries effects once per page revision and opens the tagged box without changing selection', async () => {
    install(undefined, ['one', 'two'])
    vi.mocked(commands.getBoxEffects).mockResolvedValue([false, true])
    editor()
    expect(await screen.findByRole('button', { name: 'Text effects for two' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Text effects for one' })).not.toBeInTheDocument()
    expect(commands.getBoxEffects).toHaveBeenCalledExactlyOnceWith(['one', 'two'])
    await userEvent.click(screen.getByRole('button', { name: 'Text effects for two' }))
    expect(useKoharuStore.getState().selectedLayers).toEqual(['one', 'two'])
    expect(screen.getByRole('dialog')).toHaveAccessibleName('Text Effects · two')
    await userEvent.click(screen.getByRole('button', { name: 'Reset to lettering default' }))
    expect(commands.resetBoxEffects).toHaveBeenCalledWith(['two'])
    act(() =>
      queryClient.setQueryData(projectKey, { name: 'chapter', revision: 2, active_page: 'page' }),
    )
    await waitFor(() => expect(commands.getBoxEffects).toHaveBeenCalledTimes(2))
  })

  it('shows a mixed fill mode and explicitly enables gradients for all targets', async () => {
    const gradient = {
      kind: 'Linear' as const,
      angle_degrees: 45,
      stops: [
        { offset: 0, color: [255, 0, 0, 255] as [number, number, number, number] },
        { offset: 1, color: [0, 0, 255, 255] as [number, number, number, number] },
      ],
    }
    const { update } = install(
      [text('one', 10, { fill_gradient: gradient }), text('two', 100)],
      ['one', 'two'],
    )
    editor()
    await userEvent.click(screen.getByRole('tab', { name: 'Fill' }))
    const mode = screen.getByRole('combobox', { name: 'Fill' })
    expect(mode).toHaveTextContent('Mixed')
    expect(screen.queryByRole('combobox', { name: 'Gradient kind' })).not.toBeInTheDocument()
    await userEvent.click(mode)
    await userEvent.click(await screen.findByRole('option', { name: 'Gradient' }))
    await waitFor(() => expect(update).toHaveBeenCalledTimes(1))
    expect(update.mock.calls[0][0][0].typography.fill_gradient).toEqual(gradient)
    expect(update.mock.calls[0][0][1].typography.fill_gradient?.stops[0].color).toEqual([
      0, 0, 0, 255,
    ])
    expect(await screen.findByRole('combobox', { name: 'Gradient kind' })).toBeInTheDocument()
  })

  it('lights the question tag with a count and opens directly on QC', async () => {
    install()
    issues.set('one', 2)
    editor()
    const tag = screen.getByRole('button', { name: 'Box details for one: 2 QC issues' })
    expect(tag).toHaveAttribute('data-state', 'blink')
    await userEvent.click(tag)
    expect(screen.getByTestId('qc-component')).toHaveTextContent('one')
    expect(screen.getByRole('button', { name: 'QC' })).toHaveAttribute('aria-pressed', 'true')
  })

  it('moves source, translation and redo into details and mounts warp controls in FX', async () => {
    install()
    const translation = vi.spyOn(commands, 'setTranslation').mockResolvedValue(null)
    editor()
    await userEvent.click(screen.getByRole('button', { name: 'Box details for one' }))
    await userEvent.click(screen.getByRole('button', { name: 'Source & translation' }))
    expect(screen.getByRole('textbox', { name: 'Source text' })).toHaveAttribute('readonly')
    expect(screen.getByTestId('redo-component')).toHaveTextContent('one')
    fireEvent.change(screen.getByTestId('edit-translation-one'), { target: { value: 'Edited' } })
    fireEvent.blur(screen.getByTestId('edit-translation-one'))
    await waitFor(() => expect(translation).toHaveBeenCalledWith('one', 'Edited'))
    await userEvent.click(screen.getByRole('button', { name: 'Close' }))
    act(() => useRedesignStore.getState().openFx(['one', 'two']))
    await userEvent.click(screen.getByRole('button', { name: 'Skew / perspective' }))
    expect(screen.getByTestId('warp-component')).toHaveTextContent('one,two')
  })

  it('selects an unselected right-clicked box and shows only the permitted actions', async () => {
    install()
    vi.mocked(commands.getBoxActions).mockResolvedValue({ can_split: true, bubble_boxes: 2 })
    editor(true)
    fireEvent.contextMenu(screen.getByTestId('surface'), { button: 2, clientX: 110, clientY: 30 })
    expect(useKoharuStore.getState().selectedLayers).toEqual(['two'])
    const menu = await screen.findByRole('menu')
    await waitFor(() => expect(within(menu).getAllByRole('menuitem')).toHaveLength(4))
    expect(within(menu).getByRole('menuitem', { name: 'Join with bubble' })).toBeInTheDocument()
    fireEvent.keyDown(window, { key: 'Escape' })
    await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument())
  })

  it('centers the selected text boxes through one revision-bound command', async () => {
    install(
      [text('one', 10), text('two', 100)].map((layer) => ({
        ...layer,
        automatic_region: 'bubble',
      })),
      ['one', 'two'],
      [
        {
          id: 'bubble',
          parent: 'page',
          kind: 'dev.koharu.region.bubble',
          label: null,
          geometry: {
            points: [
              { x: 0, y: 0 },
              { x: 200, y: 100 },
            ],
          },
        },
      ],
    )
    const center = vi.spyOn(commands, 'centerInBubble').mockResolvedValue(2)
    editor(true)
    fireEvent.contextMenu(screen.getByTestId('surface'), { button: 2, clientX: 20, clientY: 30 })
    const menu = await screen.findByRole('menu')
    await userEvent.click(
      within(menu).getByRole('menuitem', { name: translations.canvasAids.centerInBubble }),
    )
    await waitFor(() => expect(center).toHaveBeenCalledExactlyOnceWith(1, ['one', 'two']))
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
  })

  it('keeps a multi-selection for canvas FX, resets all targets, and hides disallowed Join', async () => {
    install(undefined, ['one', 'two'])
    vi.mocked(commands.getBoxActions).mockResolvedValue({ can_split: true, bubble_boxes: 1 })
    editor(true)
    fireEvent.contextMenu(screen.getByTestId('surface'), { button: 2, clientX: 20, clientY: 30 })
    const menu = await screen.findByRole('menu')
    await waitFor(() => expect(within(menu).getAllByRole('menuitem')).toHaveLength(3))
    expect(
      within(menu).queryByRole('menuitem', { name: 'Join with bubble' }),
    ).not.toBeInTheDocument()
    await userEvent.click(within(menu).getByRole('menuitem', { name: 'FX…' }))
    expect(screen.getByRole('dialog')).toHaveAccessibleName('Text Effects · 2 boxes')
    await userEvent.click(screen.getByRole('button', { name: 'Reset to lettering default' }))
    expect(commands.resetBoxEffects).toHaveBeenCalledWith(['one', 'two'])
  })

  it('blocks compare and active gestures, ignores empty space and middle clicks, and closes on wheel', async () => {
    install()
    editor(true)
    const surface = screen.getByTestId('surface')
    fireEvent.contextMenu(surface, { button: 2, clientX: 250, clientY: 150 })
    fireEvent.contextMenu(surface, { button: 1, clientX: 20, clientY: 30 })
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
    act(() => useCompareStore.getState().setHeld(true))
    fireEvent.contextMenu(surface, { button: 2, clientX: 20, clientY: 30 })
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
    act(() => useCompareStore.getState().setHeld(false))
    fireEvent.pointerDown(surface, { button: 0, pointerId: 1 })
    fireEvent.contextMenu(surface, { button: 2, buttons: 3, clientX: 20, clientY: 30 })
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
    fireEvent.pointerUp(surface, { button: 0, pointerId: 1 })
    fireEvent.contextMenu(surface, { button: 2, clientX: 20, clientY: 30 })
    await screen.findByRole('menu')
    fireEvent.wheel(window, { deltaY: 100 })
    await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument())
    fireEvent.contextMenu(surface, { button: 2, clientX: 20, clientY: 30 })
    await screen.findByRole('menu')
    await userEvent.click(document.body)
    await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument())
  })
})
