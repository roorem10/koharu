import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

import { LayersPanel } from '@/components/editor/LayersPanel'
import { StoryPanel } from '@/components/editor/StoryPanel'
import { effectsKey, joinNames } from '@/lib/booharu/effects'
import { openFxOn, useFx } from '@/lib/booharu/fx'
import { useBooharuQc } from '@/lib/booharu/qc'
import { queryClient } from '@/lib/queries'
import { useKoharuStore, type CanvasTool } from '@/lib/store'
import { commands, type Layer, type TextEffects } from '@koharu/ipc/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

import { boxesPage, desktop, openProject } from './boxes'
import { finding, forgetFindings } from './qc'

/** The boxes page's text effects as the desktop reads them: "second" with a second border, a
 * gradient and a skew, and the caption with a border of its own. */
const EFFECTS: TextEffects[] = [
  { layer: 'second', effects: ['second_border', 'gradient', 'skew'], skew: { x: 20, y: 0 } },
  { layer: 'caption', effects: ['border'], skew: null },
]

function mount(ui: ReactNode) {
  render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delay={0}>{ui}</TooltipProvider>
    </QueryClientProvider>,
  )
}

/** The boxes page open, the canvas showing `shown`, its text effects read, and `tool` on. */
function open(shown: string | null = 'page', tool: CanvasTool = 'select') {
  const calls = desktop()
  openProject(boxesPage(), shown)
  queryClient.setQueryData(effectsKey('page'), EFFECTS)
  useKoharuStore.setState({ tool, selectedLayers: [], selectedPages: [] })
  return calls
}

const fxTags = () =>
  screen
    .queryAllByRole('button', { name: /^Text effects for/ })
    .map((tag) => tag.getAttribute('aria-label'))

describe('Booharu fx tag (S12-j)', () => {
  it('shows on a Story row only while its text carries a text effect, beside its ? tag', () => {
    open()
    mount(<StoryPanel />)

    expect(fxTags()).toEqual(['Text effects for lo', 'Text effects for Caption'])
    const tag = screen.getByRole('button', { name: 'Text effects for lo' })
    expect(tag).toHaveTextContent('fx')
    expect(tag).toHaveClass('italic')
    // Under the ? tag in the row's column.
    expect(screen.getByRole('button', { name: 'Box details for lo' }).nextElementSibling).toBe(tag)
  })

  it('shows on a Layers row while its text carries an effect, with or without its ? tag', () => {
    open()
    forgetFindings()
    act(() =>
      useBooharuQc.setState({ project: 'Demo', pages: { page: [finding('page', 'caption')] } }),
    )
    mount(<LayersPanel />)

    // Layers lists its rows topmost first, as Koharu 0.92.0 does: the caption, last in the
    // scene, above the panel's blocks.
    expect(fxTags()).toEqual(['Text effects for caption', 'Text effects for second'])
    // "second" has no open finding: its fx tag alone shows.
    expect(screen.queryByRole('button', { name: /^Box details for second/ })).toBeNull()
    expect(
      screen.getByRole('button', { name: 'Box details for caption: 1 QC issue' }),
    ).toBeVisible()
    // A text with neither shows no tag.
    expect(screen.queryByRole('button', { name: /^Box details for first/ })).toBeNull()
  })

  it('shows no tag while the page’s effects are unread or none', () => {
    desktop()
    openProject(boxesPage())
    vi.mocked(commands.booharuTextEffects).mockReturnValue(new Promise(() => undefined))
    mount(
      <>
        <StoryPanel />
        <LayersPanel />
      </>,
    )

    expect(fxTags()).toEqual([])
  })

  it('lists the text’s effects in its tooltip, as the interface language joins them', async () => {
    open()
    const user = userEvent.setup()
    mount(<StoryPanel />)

    await user.hover(screen.getByRole('button', { name: 'Text effects for lo' }))
    expect(
      await screen.findByText('Text effects: 2nd border, Gradient fill, and Skew', {
        selector: '[data-slot=tooltip-content]',
      }),
    ).toBeInTheDocument()
  })

  it('joins the names as each language lists, Wu as Mandarin', () => {
    expect(joinNames(['Border', 'Skew'], 'en-US')).toBe('Border and Skew')
    expect(joinNames(['縁取り', '傾斜'], 'ja-JP')).toBe('縁取り、傾斜')
    expect(joinNames(['描边', '倾斜'], 'wuu')).toBe(joinNames(['描边', '倾斜'], 'zh-CN'))
    expect(joinNames(['Gradient fill'], 'fr-FR')).toBe('Gradient fill')
  })

  it('selects its text, switches to the Select tool and opens Text effects, its press its own', async () => {
    open('page', 'draw')
    const row = vi.fn()
    const user = userEvent.setup()
    mount(
      <div onPointerDown={row} onClick={row}>
        <LayersPanel />
      </div>,
    )

    await user.click(screen.getByRole('button', { name: 'Text effects for second' }))

    expect(useKoharuStore.getState().selectedLayers).toEqual(['second'])
    expect(useKoharuStore.getState().tool).toBe('select')
    expect(useFx.getState().open).toBe(true)
    // Neither the row nor the panel around it heard the press.
    expect(row).not.toHaveBeenCalled()
  })

  it('keeps a type bar tool that is on, and the selection holding its text', async () => {
    open('page', 'text')
    useKoharuStore.setState({ selectedLayers: ['first', 'second'] })
    const user = userEvent.setup()
    mount(<LayersPanel />)

    await user.click(screen.getByRole('button', { name: 'Text effects for second' }))

    expect(useKoharuStore.getState().tool).toBe('text')
    expect(useKoharuStore.getState().selectedLayers).toEqual(['first', 'second'])
    expect(useFx.getState().open).toBe(true)
  })

  it('shows its text’s page first from Story, then opens Text effects', async () => {
    const { show } = open('other', 'eraser')
    const user = userEvent.setup()
    mount(<StoryPanel />)

    await user.click(screen.getByRole('button', { name: 'Text effects for Caption' }))

    await waitFor(() => expect(useFx.getState().open).toBe(true))
    expect(show).toHaveBeenCalledWith('page', null)
    expect(useKoharuStore.getState().selectedPages).toEqual(['page'])
    expect(useKoharuStore.getState().selectedLayers).toEqual(['caption'])
    expect(useKoharuStore.getState().tool).toBe('select')
  })

  it('selects several texts once their page shows, as FX… in a box’s menu asks', async () => {
    const { show } = open('other', 'marquee')

    await act(() => openFxOn('page', ['first', 'second']))

    expect(show).toHaveBeenCalledWith('page', null)
    expect(useKoharuStore.getState().selectedLayers).toEqual(['first', 'second'])
    expect(useKoharuStore.getState().tool).toBe('select')
    expect(useFx.getState().open).toBe(true)
  })

  it('selects a text the canvas hides but opens nothing, as no bar shows over it', async () => {
    desktop()
    const page = boxesPage()
    // The balloon holding "second" is hidden, and so the text in it.
    const hide = (layers: Layer[]): Layer[] =>
      layers.map((layer) =>
        layer.id === 'split'
          ? { ...layer, visible: false }
          : layer.children
            ? { ...layer, children: hide(layer.children) }
            : layer,
      )
    openProject({ ...page, layers: hide(page.layers) })
    queryClient.setQueryData(effectsKey('page'), EFFECTS)
    useKoharuStore.setState({ tool: 'draw', selectedLayers: [], selectedPages: [] })
    const user = userEvent.setup()
    mount(<LayersPanel />)

    await user.click(screen.getByRole('button', { name: 'Text effects for second' }))

    expect(useKoharuStore.getState().selectedLayers).toEqual(['second'])
    expect(useKoharuStore.getState().tool).toBe('draw')
    expect(useFx.getState().open).toBe(false)
  })

  it('opens nothing when another text is selected while its page shows', async () => {
    const { show } = open('other')
    let shown = () => {}
    show.mockReturnValue(new Promise((resolve) => (shown = () => resolve(null))))

    const opening = openFxOn('page', ['caption'])
    act(() => useKoharuStore.setState({ selectedLayers: ['free'] }))
    shown()
    await act(() => opening)

    expect(show).toHaveBeenCalledWith('page', null)
    expect(useFx.getState().open).toBe(false)
    expect(useKoharuStore.getState().selectedLayers).toEqual(['free'])
  })
})
