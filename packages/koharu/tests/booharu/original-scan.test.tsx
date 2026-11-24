import { QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { TitleBar } from '@/components/app/TitleBar'
import { CanvasWorkspace } from '@/components/editor/CanvasWorkspace'
import { LayersPanel } from '@/components/editor/LayersPanel'
import { exceptOriginalScan, isOriginalScan } from '@/lib/booharu/attrs'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import type { Layer, Page } from '@koharu/ipc/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

import { block, desktop, openProject } from './boxes'
import { settle } from './reader'

/** An image layer as Fix page levels keeps a page's scan in it: hidden, locked, at the bottom, and
 * named as the scan's file was. */
function originalScan(id = 'original', flag: unknown = true): Layer {
  return {
    id,
    name: 'scan-001.png',
    visible: false,
    locked: true,
    opacity: 1,
    blend: 'normal',
    attributes: { booharu: { original_scan: flag } },
    kind: 'image',
    file: 'hash',
  } as Layer
}

/** A levelled page: its Original scan under two boxes. */
function levelledPage(): Page {
  return {
    id: 'page',
    size: { width: 800, height: 1200 },
    background: null,
    layers: [originalScan(), block('first', 'Hello'), block('second', 'there')],
  }
}

afterEach(() => {
  cleanup()
  document.body.replaceChildren()
})

describe('The Original scan in the interface', () => {
  it('is an image layer whose original_scan is true, and nothing else', () => {
    expect(isOriginalScan(originalScan())).toBe(true)
    expect(isOriginalScan(originalScan('other', 'yes'))).toBe(false)
    expect(
      isOriginalScan({ ...block('box', 'Hi'), attributes: { booharu: { original_scan: true } } }),
    ).toBe(false)
    expect(exceptOriginalScan(levelledPage().layers).map((layer) => layer.id)).toEqual([
      'first',
      'second',
    ])
  })

  it('is left out of Select all on the canvas', async () => {
    desktop()
    // jsdom has no 2D canvas; the workspace then sends black for its theme colours.
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null)
    openProject(levelledPage())
    render(
      <QueryClientProvider client={queryClient}>
        <CanvasWorkspace />
      </QueryClientProvider>,
    )
    await settle()
    act(() => {
      fireEvent.keyDown(document.body, { key: 'a', code: 'KeyA', ctrlKey: true })
    })
    expect(useKoharuStore.getState().selectedLayers).toEqual(['first', 'second'])
  })

  it("is left out of the Edit menu's Select all layers, in Read mode too", async () => {
    desktop()
    openProject(levelledPage())
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <TitleBar />
      </QueryClientProvider>,
    )
    for (const reading of [false, true]) {
      act(() => useKoharuStore.setState({ reading, selectedLayers: [] }))
      await user.click(screen.getByRole('menuitem', { name: 'Edit' }))
      await user.click(await screen.findByRole('menuitem', { name: /^Select All Layers/ }))
      expect(useKoharuStore.getState().selectedLayers, String(reading)).toEqual(['first', 'second'])
    }
  })

  it('is named Original scan in the Layers panel, as its bottom row', async () => {
    desktop()
    openProject(levelledPage())
    render(
      <QueryClientProvider client={queryClient}>
        <TooltipProvider delay={0}>
          <LayersPanel />
        </TooltipProvider>
      </QueryClientProvider>,
    )
    const rows = await screen.findAllByRole('button', { name: /^Edit / })
    expect(rows.at(-1)).toHaveAccessibleName('Edit Original scan')
    expect(screen.queryByText('scan-001.png')).toBeNull()
  })
})
