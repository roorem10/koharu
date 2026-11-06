import { fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { CanvasViewAids } from '@/components/editor/CanvasViewAids'
import { useCanvasViewStore } from '@/components/editor/viewAids'
import { useKoharuStore } from '@/lib/store'

describe('canvas guides', () => {
  afterEach(() => vi.unstubAllGlobals())

  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(
      new DOMRect(0, 0, 400, 300),
    )
    vi.stubGlobal('devicePixelRatio', 2)
    useKoharuStore.setState({ camera: { zoom: 4, translation: [20, 40], fitted: false } })
    useCanvasViewStore.setState({ rulers: true, grid: false, guides: [] })
  })

  it('creates only on drop and converts screen position through zoom, pan, and pixel ratio', () => {
    render(<CanvasViewAids size={{ width: 200, height: 200 }} />)
    const ruler = screen.getByTestId('horizontal-ruler')
    fireEvent.pointerDown(ruler, { pointerId: 1, button: 0, clientX: 50, clientY: 10 })
    fireEvent.pointerMove(ruler, { pointerId: 1, clientX: 50, clientY: 100 })
    expect(useCanvasViewStore.getState().guides).toEqual([])
    fireEvent.pointerUp(ruler, { pointerId: 1, clientX: 50, clientY: 100 })
    expect(useCanvasViewStore.getState().guides).toEqual([
      { id: expect.any(String), axis: 'y', position: 40 },
    ])
  })

  it('cancels new and existing guide drags without changing committed guides', () => {
    useCanvasViewStore.setState({ guides: [{ id: 'existing', axis: 'x', position: 30 }] })
    const { container } = render(<CanvasViewAids size={{ width: 200, height: 200 }} />)
    const guide = container.querySelector('[data-guide="existing"]')!
    fireEvent.pointerDown(guide, { pointerId: 2, button: 0, clientX: 70, clientY: 100 })
    fireEvent.pointerMove(guide, { pointerId: 2, clientX: 140, clientY: 100 })
    fireEvent.pointerCancel(guide, { pointerId: 2 })
    expect(useCanvasViewStore.getState().guides).toEqual([
      { id: 'existing', axis: 'x', position: 30 },
    ])
    const ruler = screen.getByTestId('vertical-ruler')
    fireEvent.pointerDown(ruler, { pointerId: 3, button: 0, clientX: 10, clientY: 100 })
    fireEvent.pointerMove(ruler, { pointerId: 3, clientX: 100, clientY: 100 })
    fireEvent.keyDown(window, { key: 'Escape' })
    fireEvent.pointerUp(ruler, { pointerId: 3, clientX: 100, clientY: 100 })
    expect(useCanvasViewStore.getState().guides).toHaveLength(1)
  })

  it('moves existing guides and deletes them only after a drop outside the canvas', () => {
    useCanvasViewStore.setState({ guides: [{ id: 'existing', axis: 'x', position: 30 }] })
    const { container } = render(<CanvasViewAids size={{ width: 200, height: 200 }} />)
    const guide = container.querySelector('[data-guide="existing"]')!
    fireEvent.pointerDown(guide, { pointerId: 4, button: 0, clientX: 70, clientY: 100 })
    fireEvent.pointerUp(guide, { pointerId: 4, clientX: 110, clientY: 100 })
    expect(useCanvasViewStore.getState().guides[0].position).toBe(50)
    fireEvent.pointerDown(guide, { pointerId: 5, button: 0, clientX: 110, clientY: 100 })
    fireEvent.pointerMove(guide, { pointerId: 5, clientX: -10, clientY: 100 })
    expect(useCanvasViewStore.getState().guides).toHaveLength(1)
    fireEvent.pointerUp(guide, { pointerId: 5, clientX: -10, clientY: 100 })
    expect(useCanvasViewStore.getState().guides).toEqual([])
  })

  it('keeps guides inactive while the on-page editor owns canvas gestures', () => {
    render(
      <CanvasViewAids
        size={{ width: 200, height: 200 }}
        disabled
        snapLines={[{ axis: 'x', position: 20 }]}
      />,
    )
    const ruler = screen.getByTestId('horizontal-ruler')
    fireEvent.pointerDown(ruler, { pointerId: 6, button: 0, clientX: 50, clientY: 10 })
    fireEvent.pointerUp(ruler, { pointerId: 6, clientX: 50, clientY: 100 })
    expect(useCanvasViewStore.getState().guides).toEqual([])
    expect(screen.queryByTestId('snap-line')).not.toBeInTheDocument()
  })

  it('preserves the grab offset so clicking a guide hit strip does not move the guide', () => {
    useCanvasViewStore.setState({ guides: [{ id: 'existing', axis: 'x', position: 30 }] })
    const { container } = render(<CanvasViewAids size={{ width: 200, height: 200 }} />)
    const guide = container.querySelector('[data-guide="existing"]')!
    fireEvent.pointerDown(guide, { pointerId: 7, button: 0, clientX: 72, clientY: 100 })
    fireEvent.pointerUp(guide, { pointerId: 7, clientX: 72, clientY: 100 })
    expect(useCanvasViewStore.getState().guides[0].position).toBe(30)
    fireEvent.pointerDown(guide, { pointerId: 8, button: 0, clientX: 72, clientY: 100 })
    fireEvent.pointerMove(guide, { pointerId: 8, clientX: 112, clientY: 100 })
    fireEvent.pointerUp(guide, { pointerId: 8, clientX: 112, clientY: 100 })
    expect(useCanvasViewStore.getState().guides[0].position).toBe(50)
  })

  it.each(['draw', 'eraser', 'pan'] as const)(
    'lets %s gestures pass through existing guides while rulers can create guides',
    (tool) => {
      useCanvasViewStore.setState({ guides: [{ id: 'existing', axis: 'x', position: 30 }] })
      useKoharuStore.setState({ tool })
      const pointerDown = vi.fn()
      const { container } = render(
        <div onPointerDown={pointerDown}>
          <CanvasViewAids size={{ width: 200, height: 200 }} />
        </div>,
      )
      const guide = container.querySelector('[data-guide="existing"]')!
      expect(guide).toHaveStyle({ pointerEvents: 'none' })
      fireEvent.pointerDown(guide, { pointerId: 9, button: 0, clientX: 70, clientY: 100 })
      fireEvent.pointerUp(guide, { pointerId: 9, clientX: 110, clientY: 100 })
      expect(pointerDown).toHaveBeenCalledOnce()
      expect(useCanvasViewStore.getState().guides[0].position).toBe(30)
      const ruler = screen.getByTestId('horizontal-ruler')
      fireEvent.pointerDown(ruler, { pointerId: 10, button: 0, clientX: 50, clientY: 10 })
      fireEvent.pointerUp(ruler, { pointerId: 10, clientX: 50, clientY: 100 })
      expect(useCanvasViewStore.getState().guides).toHaveLength(2)
      expect(pointerDown).toHaveBeenCalledOnce()
    },
  )
})
