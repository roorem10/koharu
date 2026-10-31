import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { WarpHandles } from '@/components/editor/WarpHandles'
import { useCompareStore } from '@/components/qc/compare'
import { unitCorners } from '@/components/warp/geometry'
import { TextWarpControls } from '@/components/warp/TextWarpControls'
import { pageKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Layer, type TextWarp } from '@koharu/bridge/protocol'

const project = { name: 'Warp', revision: 1, active_page: 'page', can_undo: false, can_redo: false }
const layer: Layer = {
  type: 'text',
  id: 'box',
  parent: 'page',
  geometry: null,
  angle_degrees: 0,
  visibility: { visible: true, opacity: 1 },
  content: {
    id: 'content',
    source: null,
    translation: { text: 'Hello', language: null },
    role: null,
    source_region: null,
  },
  typography: null,
  layout: 'paragraph',
  automatic_region: null,
}
const page = {
  id: 'page',
  label: 'Page',
  size: { width: 200, height: 150 },
  layers: [layer],
  regions: [],
}
let values: (TextWarp | null)[]

beforeEach(() => {
  useCompareStore.getState().reset()
  values = [null]
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  vi.spyOn(commands, 'getPage').mockResolvedValue(page)
  vi.spyOn(commands, 'getTextWarp').mockImplementation(async (layers) =>
    layers.map((_, index) => values[index] ?? null),
  )
  vi.spyOn(commands, 'setTextWarp').mockImplementation(async (layers, warp) => {
    values = layers.map(() => warp)
    return null
  })
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pageKey, page)
  useKoharuStore.setState({
    camera: { zoom: 2, translation: [0, 0], fitted: false },
    canvasRevision: 1,
    layerFrames: { box: { x: 10, y: 20, width: 100, height: 50, angle_degrees: 0 } },
  })
})
function mount(children: React.ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>)
}
async function mode(name: string) {
  const user = userEvent.setup()
  const select = await screen.findByRole('combobox', { name: 'Warp mode' })
  await waitFor(() => expect(select).toBeEnabled())
  await user.click(select)
  await user.click(await screen.findByRole('option', { name }))
}

describe('text warp controls', () => {
  it('switches modes for all selected boxes and resets perspective corners', async () => {
    values = [null, null]
    mount(<TextWarpControls layers={['box', 'other']} />)
    await mode('Skew')
    await waitFor(() =>
      expect(commands.setTextWarp).toHaveBeenLastCalledWith(['box', 'other'], {
        kind: 'skew',
        x_degrees: 0,
        y_degrees: 0,
      }),
    )
    const horizontal = await screen.findByLabelText('Horizontal (°)')
    const user = userEvent.setup()
    await user.clear(horizontal)
    await user.type(horizontal, '20')
    await user.tab()
    await waitFor(() =>
      expect(commands.setTextWarp).toHaveBeenLastCalledWith(['box', 'other'], {
        kind: 'skew',
        x_degrees: 20,
        y_degrees: 0,
      }),
    )
    await mode('Perspective')
    await waitFor(() =>
      expect(commands.setTextWarp).toHaveBeenLastCalledWith(['box', 'other'], {
        kind: 'perspective',
        corners: unitCorners(),
      }),
    )
    const reset = await screen.findByRole('button', { name: 'Reset corners' })
    await waitFor(() => expect(reset).toBeEnabled())
    await user.click(reset)
    expect(commands.setTextWarp).toHaveBeenLastCalledWith(['box', 'other'], {
      kind: 'perspective',
      corners: unitCorners(),
    })
    await mode('None')
    await waitFor(() =>
      expect(commands.setTextWarp).toHaveBeenLastCalledWith(['box', 'other'], null),
    )
  })
  it('loads fresh values after undo and redo canvas revisions', async () => {
    mount(<TextWarpControls layers={['box']} />)
    await waitFor(() => expect(screen.getByRole('combobox')).toHaveTextContent('None'))
    values = [{ kind: 'skew', x_degrees: 30, y_degrees: 0 }]
    act(() => useKoharuStore.setState({ canvasRevision: 2 }))
    await screen.findByLabelText('Horizontal (°)')
    expect(screen.getByLabelText('Horizontal (°)')).toHaveValue('30')
    values = [null]
    act(() => useKoharuStore.setState({ canvasRevision: 3 }))
    await waitFor(() => expect(screen.getByRole('combobox')).toHaveTextContent('None'))
    expect(commands.setTextWarp).not.toHaveBeenCalled()
  })
  it('shows a mixed selection and refuses edits if loading fails', async () => {
    values = [null, { kind: 'skew', x_degrees: 10, y_degrees: 0 }]
    const view = mount(<TextWarpControls layers={['box', 'other']} />)
    await waitFor(() => expect(screen.getByRole('combobox')).toHaveTextContent('Mixed'))
    await mode('Skew')
    await waitFor(() => expect(commands.setTextWarp).toHaveBeenCalledTimes(1))
    vi.mocked(commands.getTextWarp).mockRejectedValueOnce(new Error('Missing box'))
    view.rerender(
      <QueryClientProvider client={queryClient}>
        <TextWarpControls layers={['stale']} />
      </QueryClientProvider>,
    )
    await screen.findByRole('alert')
    expect(screen.getByRole('combobox')).toBeDisabled()
  })
})

describe('warp canvas overlay', () => {
  it('previews four perspective handles and commits once on release with camera conversion', async () => {
    values = [{ kind: 'perspective', corners: unitCorners() }]
    mount(<WarpHandles layerId='box' />)
    const handle = await screen.findByRole('button', { name: 'Perspective corner 1' })
    expect(screen.getAllByRole('button')).toHaveLength(4)
    expect(handle).toHaveStyle({ left: '20px', top: '40px' })
    fireEvent.pointerDown(handle, { button: 0, pointerId: 3, clientX: 20, clientY: 40 })
    fireEvent.pointerMove(handle, { pointerId: 3, clientX: 40, clientY: 50 })
    expect(commands.setTextWarp).not.toHaveBeenCalled()
    fireEvent.pointerUp(handle, { pointerId: 3, clientX: 40, clientY: 50 })
    fireEvent.lostPointerCapture(handle, { pointerId: 3 })
    await waitFor(() => expect(commands.setTextWarp).toHaveBeenCalledTimes(1))
    const warp = vi.mocked(commands.setTextWarp).mock.calls[0][1]
    expect(warp?.kind).toBe('perspective')
    if (warp?.kind === 'perspective') {
      expect(warp.corners[0][0]).toBeCloseTo(0.1)
      expect(warp.corners[0][1]).toBeCloseTo(0.1)
      expect(warp.corners.slice(1)).toEqual(unitCorners().slice(1))
    }
  })
  it('discards canceled and self-crossing drags', async () => {
    values = [{ kind: 'perspective', corners: unitCorners() }]
    mount(<WarpHandles layerId='box' />)
    const handle = await screen.findByRole('button', { name: 'Perspective corner 1' })
    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 20, clientY: 40 })
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 40, clientY: 50 })
    fireEvent.pointerCancel(handle, { pointerId: 1 })
    fireEvent.pointerUp(handle, { pointerId: 1, clientX: 40, clientY: 50 })
    fireEvent.pointerDown(handle, { button: 0, pointerId: 2, clientX: 20, clientY: 40 })
    fireEvent.pointerUp(handle, { pointerId: 2, clientX: 300, clientY: 180 })
    expect(commands.setTextWarp).not.toHaveBeenCalled()
  })
  it('draws only an outline for skew and hides controls in compare mode or None', async () => {
    values = [{ kind: 'skew', x_degrees: 30, y_degrees: 0 }]
    mount(<WarpHandles layerId='box' />)
    await screen.findByTestId('warp-outline')
    expect(screen.queryAllByRole('button')).toHaveLength(0)
    act(() => useCompareStore.getState().toggle())
    expect(screen.queryByTestId('warp-outline')).not.toBeInTheDocument()
    act(() => useCompareStore.getState().reset())
    values = [null]
    act(() => useKoharuStore.setState({ canvasRevision: 2 }))
    await waitFor(() => expect(screen.queryByTestId('warp-outline')).not.toBeInTheDocument())
  })
})
