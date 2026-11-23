import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { BoxDetailsDialog } from '@/components/booharu/details/BoxDetailsDialog'
import { LayersPanel } from '@/components/editor/LayersPanel'
import { StoryPanel } from '@/components/editor/StoryPanel'
import { useBoxDetails } from '@/lib/booharu/boxDetails'
import { useBooharuQc } from '@/lib/booharu/qc'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import type { Finding } from '@koharu/ipc/protocol'

import { block, boxesPage, desktop, openProject } from './boxes'
import { finding, forgetFindings } from './qc'

const overflow = finding('page', 'second')
const flag = finding('page', 'second', { kind: 'flag', message: 'Check the name' })
const small = finding(
  'page',
  'second',
  { kind: 'too_small', size: 9.5, limit: 12 },
  { hidden: true },
)
const caption = finding('page', 'caption', { kind: 'too_small', size: 8, limit: 12 })
const marked = finding('page', 'first', { kind: 'empty_source' }, { hidden: true })

/** The findings the desktop told for the boxes page, as the store holds them. */
function hold(findings: Finding[]) {
  act(() => useBooharuQc.setState({ project: 'Demo', pages: { page: findings } }))
}

function mount(ui: ReactNode) {
  render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>)
}

/** Box details, once it shows. */
function details(): Promise<HTMLElement> {
  return screen.findByRole('dialog', { name: 'Box details' })
}

/** Box details' QC section. */
async function qcSection() {
  return within(within(await details()).getByRole('region', { name: 'QC' }))
}

beforeEach(() => {
  forgetFindings()
  useBoxDetails.setState({ shown: null, reveal: null })
})

describe('Booharu QC ? tag', () => {
  it('turns a Story row’s tag yellow and blinking while its text has open findings', async () => {
    desktop()
    const page = boxesPage()
    // A block whose source is blank, as an empty OCR result leaves it, has no name to give.
    page.layers[1] = block('free', '')
    openProject(page)
    hold([
      marked,
      overflow,
      flag,
      small,
      caption,
      finding('page', 'free', { kind: 'empty_source' }),
    ])
    const user = userEvent.setup()
    mount(
      <>
        <StoryPanel />
        <BoxDetailsDialog />
      </>,
    )

    // The count is the open findings, a flag among them; one marked OK isn't counted.
    const tag = screen.getByRole('button', { name: 'Box details for lo: 2 QC issues' })
    expect(tag).toHaveTextContent('?')
    expect(tag).toHaveClass('booharu-qc-tag', 'booharu-qc-blink')
    expect(screen.getByRole('button', { name: 'Box details for Caption: 1 QC issue' })).toHaveClass(
      'booharu-qc-tag',
      'booharu-qc-blink',
    )
    expect(screen.getByRole('button', { name: 'Box details: 1 QC issue' })).toHaveClass(
      'booharu-qc-blink',
    )
    // A text whose findings are all marked OK keeps the quiet tag.
    const quiet = screen.getByRole('button', { name: 'Box details for Hel' })
    expect(quiet).not.toHaveClass('booharu-qc-tag')
    expect(quiet).not.toHaveClass('booharu-qc-blink')

    await user.hover(tag)
    expect(await screen.findByText('Box details: 2 QC issues')).toBeInTheDocument()
    await user.unhover(tag)

    // It opens Box details on the QC section, which takes the focus.
    await user.click(tag)
    expect(useBoxDetails.getState().shown).toEqual({ page: 'page', layer: 'second', section: 'qc' })
    const qc = await qcSection()
    await waitFor(() =>
      expect(qc.getByRole('button', { name: 'Mark Text overflows the box OK' })).toHaveFocus(),
    )
    await user.keyboard('{Escape}')
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())

    // The quiet tag opens Box details as before.
    await user.click(quiet)
    expect(useBoxDetails.getState().shown).toEqual({ page: 'page', layer: 'first' })
    await user.keyboard('{Escape}')
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())

    // Marked OK, the text's tag is quiet again as soon as the desktop tells it.
    hold([marked, { ...overflow, hidden: true }, { ...flag, hidden: true }, small, caption])
    expect(screen.getByRole('button', { name: 'Box details for lo' })).not.toHaveClass(
      'booharu-qc-blink',
    )
  })

  it('shows on a Layers row only while its text has open findings, and a press stays its own', async () => {
    desktop()
    openProject(boxesPage())
    hold([marked, overflow, flag, small, caption])
    const row = vi.fn()
    const user = userEvent.setup()
    mount(
      <>
        <div onPointerDown={row} onClick={row}>
          <LayersPanel />
        </div>
        <BoxDetailsDialog />
      </>,
    )

    // Layers names each row by its layer's name, and lists the rows topmost first, as Koharu
    // 0.92.0 does: the caption, last in the scene, above the panel's blocks.
    expect(
      screen
        .getAllByRole('button', { name: /^Box details/ })
        .map((tag) => tag.getAttribute('aria-label')),
    ).toEqual(['Box details for caption: 1 QC issue', 'Box details for second: 2 QC issues'])
    const tag = screen.getByRole('button', { name: 'Box details for second: 2 QC issues' })
    expect(tag).toHaveClass('booharu-qc-tag', 'booharu-qc-blink')

    // Neither a drag nor the row's selection hears the press.
    await user.click(tag)
    expect(useBoxDetails.getState().shown).toEqual({ page: 'page', layer: 'second', section: 'qc' })
    expect(row).not.toHaveBeenCalled()
    expect(useKoharuStore.getState().selectedLayers).toEqual([])
    await user.keyboard('{Escape}')
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
    expect(screen.getByRole('button', { name: 'Edit second' })).toHaveAttribute(
      'aria-expanded',
      'false',
    )

    // The tag goes once nothing on its text is open.
    hold([marked, { ...overflow, hidden: true }, { ...flag, hidden: true }, small, caption])
    expect(screen.queryByRole('button', { name: /^Box details for second/ })).toBeNull()
    expect(
      screen.getByRole('button', { name: 'Box details for caption: 1 QC issue' }),
    ).toBeVisible()
  })

  it('leaves Show in Story’s scroll to Story’s row', async () => {
    desktop()
    openProject(boxesPage())
    hold([overflow])
    const scroll = vi.mocked(Element.prototype.scrollIntoView)
    mount(<LayersPanel />)
    expect(screen.getByRole('button', { name: 'Box details for second: 1 QC issue' })).toBeVisible()

    const reveal = { layer: 'second', until: performance.now() + 2000 }
    act(() => useBoxDetails.setState({ reveal }))
    await act(() => new Promise((resolve) => requestAnimationFrame(resolve)))
    expect(useBoxDetails.getState().reveal).toBe(reveal)
    expect(scroll).not.toHaveBeenCalled()
  })
})
