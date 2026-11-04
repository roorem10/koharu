import { QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { Inspector } from '@/components/editor/Inspector'
import { layerDropIndex } from '@/components/files/layer-order'
import { fontsKey, pageKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Layer, type Page } from '@koharu/bridge/protocol'

const group: Layer = {
  type: 'group',
  id: 'group',
  parent: 'page',
  visibility: { visible: true, opacity: 1 },
  role: 'text',
  name: 'Text',
}
const rows: Layer[] = ['one', 'two', 'three'].map((id) => ({
  type: 'text',
  id,
  parent: 'group',
  geometry: null,
  angle_degrees: null,
  visibility: { visible: true, opacity: 1 },
  typography: null,
  layout: 'paragraph',
  automatic_region: null,
  content: {
    id: `content-${id}`,
    source: null,
    translation: { text: id, language: null },
    role: null,
    source_region: null,
  },
}))
const page: Page = {
  id: 'page',
  label: 'Page',
  size: { width: 100, height: 100 },
  regions: [],
  layers: [group, ...rows],
}

describe('layer drag reorder', () => {
  it('commits row three above row one once and does nothing on a canceled drag', async () => {
    queryClient.setQueryData(pageKey, page)
    queryClient.setQueryData(projectKey, {
      name: 'Project',
      revision: 1,
      active_page: 'page',
      can_undo: false,
      can_redo: false,
    })
    queryClient.setQueryData(fontsKey, [])
    useKoharuStore.setState({ selectedLayers: [] })
    const move = vi.spyOn(commands, 'moveLayer').mockResolvedValue(page)
    vi.spyOn(commands, 'getProject').mockResolvedValue({
      name: 'Project',
      revision: 2,
      active_page: 'page',
      can_undo: true,
      can_redo: false,
    })
    render(
      <QueryClientProvider client={queryClient}>
        <Inspector />
      </QueryClientProvider>,
    )
    const source = screen.getByRole('button', { name: /Drag three to reorder/ })
    const target = screen
      .getByRole('button', { name: /Drag one to reorder/ })
      .closest('[data-layer-row]')!
    const transfer = { setData: vi.fn(), effectAllowed: '', dropEffect: '' }
    fireEvent.dragStart(source, { dataTransfer: transfer })
    fireEvent.dragEnd(source)
    expect(move).not.toHaveBeenCalled()
    fireEvent.dragStart(source, { dataTransfer: transfer })
    fireEvent.dragOver(target, { dataTransfer: transfer, clientY: 0 })
    fireEvent.drop(target, { dataTransfer: transfer, clientY: 0 })
    await waitFor(() => expect(move).toHaveBeenCalledExactlyOnceWith('three', 'group', 0))
  })

  it('maps reversed paint-order groups and rejects other parents and locked artwork', () => {
    const reversed: Layer[] = [{ ...group, role: null }, ...rows]
    expect(layerDropIndex(reversed, 'page', 'one', 'three', false)).toEqual({
      parent: 'group',
      index: 2,
    })
    expect(layerDropIndex(page.layers, 'page', 'three', 'one', false)).toEqual({
      parent: 'group',
      index: 0,
    })
    expect(
      layerDropIndex(
        [...page.layers, { ...rows[0]!, id: 'other', parent: 'elsewhere' }],
        'page',
        'one',
        'other',
        false,
      ),
    ).toBeNull()
    const artwork: Layer = {
      type: 'artwork',
      id: 'artwork',
      parent: 'page',
      geometry: { points: [] },
      image: '',
      visibility: { visible: true, opacity: 1 },
    }
    expect(layerDropIndex([artwork, ...rows], 'page', 'artwork', 'one', false)).toBeNull()
  })
})
