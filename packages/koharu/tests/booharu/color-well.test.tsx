import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ColorWell } from '@/components/controls/ColorWell'
import { useColorHistory } from '@/lib/colorHistory'

// A picker drag whose end event never fires (koharu#866), ported from Booharu 0.0.4: closing the
// picker applies the dragged colour, Escape discards it, and any later choice wins over it.

describe('colour well with an unfinished drag', () => {
  beforeEach(() => useColorHistory.setState({ colors: [] }))

  it('commits an unfinished black drag when the picker closes from its swatch', async () => {
    const onChange = vi.fn()
    await unfinishedBlackDrag(onChange)

    fireEvent.click(screen.getByRole('button', { name: 'Brush color' }))

    await waitFor(() => expect(onChange).toHaveBeenCalledExactlyOnceWith('#000000'))
  })

  it('commits an unfinished drag when a click outside closes the picker', async () => {
    const onChange = vi.fn()
    await unfinishedBlackDrag(onChange)

    // A click outside whose mouse events never reach the window, where the picker listens for
    // the end of its drag (a mouseup, or a move with no button held): the popover still sees the
    // whole click and closes.
    const outside = document.createElement('button')
    for (const type of ['mousemove', 'mouseup']) {
      outside.addEventListener(type, (event) => event.stopPropagation())
    }
    document.body.append(outside)
    await userEvent.setup().click(outside)

    await waitFor(() => expect(screen.queryByRole('slider', { name: 'Color' })).toBeNull())
    expect(onChange).toHaveBeenCalledExactlyOnceWith('#000000')
    outside.remove()
  })

  it('discards an unfinished drag on Escape', async () => {
    const onChange = vi.fn()
    const picker = await unfinishedBlackDrag(onChange)

    fireEvent.keyDown(picker, { key: 'Escape' })

    await waitFor(() => expect(screen.queryByRole('slider', { name: 'Color' })).toBeNull())
    expect(onChange).not.toHaveBeenCalled()
    expect(screen.getByRole('button', { name: 'Brush color' }).firstElementChild).toHaveStyle({
      backgroundColor: '#111111',
    })
  })

  it('does not replay an unfinished drag after choosing Transparent', async () => {
    const onChange = vi.fn()
    await unfinishedBlackDrag(onChange, true)

    fireEvent.click(screen.getByRole('button', { name: 'Transparent' }))
    fireEvent.click(screen.getByRole('button', { name: 'Brush color' }))

    await waitFor(() => expect(onChange).toHaveBeenCalledExactlyOnceWith(null))
  })

  it('does not replay an unfinished drag after committing a hex code', async () => {
    const onChange = vi.fn()
    await unfinishedBlackDrag(onChange)

    fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
      target: { value: '#ABCDEF' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Brush color' }))

    await waitFor(() => expect(onChange).toHaveBeenCalledExactlyOnceWith('#ABCDEF'))
  })

  it('closes with nothing to apply after a drag that ended normally', async () => {
    const onChange = vi.fn()
    const picker = await unfinishedBlackDrag(onChange)
    fireEvent.mouseUp(window)
    await waitFor(() => expect(onChange).toHaveBeenCalledExactlyOnceWith('#000000'))

    fireEvent.click(screen.getByRole('button', { name: 'Brush color' }))

    await waitFor(() => expect(picker).not.toBeInTheDocument())
    expect(onChange).toHaveBeenCalledOnce()
  })
})

// Koharu 0.90.0's Recent colors keep the colour a well closes on; Booharu's unfinished drag joins
// them only when closing applies it.
describe('colour well with recent colours and an unfinished drag', () => {
  beforeEach(() => useColorHistory.setState({ colors: [] }))

  it('remembers an unfinished drag that closing applies', async () => {
    const onChange = vi.fn()
    await unfinishedBlackDrag(onChange)

    fireEvent.click(screen.getByRole('button', { name: 'Brush color' }))

    await waitFor(() => expect(onChange).toHaveBeenCalledExactlyOnceWith('#000000'))
    expect(useColorHistory.getState().colors).toEqual(['#000000'])
  })

  it('remembers nothing when Escape discards an unfinished drag', async () => {
    const onChange = vi.fn()
    const picker = await unfinishedBlackDrag(onChange)

    fireEvent.keyDown(picker, { key: 'Escape' })

    await waitFor(() => expect(screen.queryByRole('slider', { name: 'Color' })).toBeNull())
    expect(onChange).not.toHaveBeenCalled()
    expect(useColorHistory.getState().colors).toEqual([])
  })

  it('lets a recent colour settle an unfinished drag', async () => {
    useColorHistory.setState({ colors: ['#ABCDEF'] })
    const onChange = vi.fn()
    await unfinishedBlackDrag(onChange)

    fireEvent.click(screen.getByRole('button', { name: 'Recent color #ABCDEF' }))
    fireEvent.click(screen.getByRole('button', { name: 'Brush color' }))

    await waitFor(() => expect(screen.queryByRole('slider', { name: 'Color' })).toBeNull())
    expect(onChange).toHaveBeenCalledExactlyOnceWith('#ABCDEF')
    expect(useColorHistory.getState().colors).toEqual(['#ABCDEF'])
  })

  it('remembers a colour closed on with no drag waiting, as Koharu does', async () => {
    const onChange = vi.fn()
    render(<ColorWell value='#111111' onChange={onChange} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Brush color' }))

    fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
      target: { value: '#ABCDEF' },
    })
    fireEvent.keyDown(screen.getByRole('textbox', { name: 'Hex color code' }), { key: 'Escape' })

    await waitFor(() => expect(screen.queryByRole('slider', { name: 'Color' })).toBeNull())
    expect(onChange).toHaveBeenCalledExactlyOnceWith('#ABCDEF')
    expect(useColorHistory.getState().colors).toEqual(['#ABCDEF'])
  })
})

/** Opens the well at #111111 and drags its picker to black without the drag's end event. */
async function unfinishedBlackDrag(
  onChange: (color: string | null) => void,
  allowTransparent = false,
) {
  render(
    allowTransparent ? (
      <ColorWell value='#111111' allowTransparent onChange={onChange} />
    ) : (
      <ColorWell value='#111111' onChange={onChange} />
    ),
  )
  await userEvent.setup().click(screen.getByRole('button', { name: 'Brush color' }))
  const picker = screen.getByRole('slider', { name: 'Color' })
  Object.defineProperty(picker, 'getBoundingClientRect', {
    value: () => ({ left: 0, top: 0, width: 200, height: 160 }),
  })
  // No mouseup: the drag's end event is lost, as it was on Windows.
  fireEvent.mouseDown(picker, { buttons: 1, clientX: 0, clientY: 160 })
  await waitFor(() =>
    expect(picker).toHaveAttribute('aria-valuetext', 'Saturation 0%, Brightness 0%'),
  )
  expect(onChange).not.toHaveBeenCalled()
  return picker
}
