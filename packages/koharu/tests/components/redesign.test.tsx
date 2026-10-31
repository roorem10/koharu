import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { useCanvasContextMenu } from '@/components/editor/CanvasContextMenu'
import { Inspector } from '@/components/editor/Inspector'
import { useCompareStore } from '@/components/qc/compare'
import { useRedesignStore } from '@/components/redesign/state'
import { defaultFont, defaultTypography } from '@/components/redesign/typography'
import { fontsKey, pageKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
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

function install(layers = [text('one', 10), text('two', 100)], selected = ['one']) {
  issues.clear()
  useCompareStore.getState().reset()
  let page: Page = {
    id: 'page',
    label: 'Page',
    size: { width: 300, height: 200 },
    layers,
    regions: [],
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
