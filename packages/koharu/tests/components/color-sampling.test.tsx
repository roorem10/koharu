import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ColorSamplingProvider, useColorSampling } from '@/components/controls/ColorSampling'
import { ColorWell } from '@/components/controls/ColorWell'
import { recentColorsKey } from '@/components/controls/recent-colors'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/bridge/protocol'

describe('canvas color sampling', () => {
  beforeEach(() => useKoharuStore.setState({ tool: 'text' }))

  it('applies a canvas sample to the requesting color well and restores the previous tool', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()

    render(
      <ColorSamplingProvider>
        <ColorWell value='#111111' onChange={onChange} />
        <CompleteSample />
      </ColorSamplingProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Brush color' }))
    await user.click(screen.getByRole('button', { name: 'Pick color from canvas' }))
    expect(useKoharuStore.getState().tool).toBe('color_picker')

    await user.click(screen.getByRole('button', { name: 'Complete sample' }))
    expect(onChange).toHaveBeenCalledWith('#123456')
    await waitFor(() => expect(commands.rememberColor).toHaveBeenCalledWith([18, 52, 86, 255]))
    expect(useKoharuStore.getState().tool).toBe('text')
  })

  it('commits shorthand white as a full RGB color', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<ColorWell value='#111111' onChange={onChange} />)

    await user.click(screen.getByRole('button', { name: 'Brush color' }))
    fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
      target: { value: '#FFF' },
    })

    expect(onChange).toHaveBeenCalledWith('#FFFFFF')
  })

  it('commits the final picker color when its window-level drag ends', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<ColorWell value='#111111' onChange={onChange} />)

    await user.click(screen.getByRole('button', { name: 'Brush color' }))
    const picker = screen.getByRole('slider', { name: 'Color' })
    Object.defineProperty(picker, 'getBoundingClientRect', {
      value: () => ({ left: 0, top: 0, width: 200, height: 160 }),
    })
    fireEvent.mouseDown(picker, { buttons: 1, clientX: 0, clientY: 0 })
    await waitFor(() =>
      expect(picker).toHaveAttribute('aria-valuetext', 'Saturation 0%, Brightness 100%'),
    )
    fireEvent.mouseUp(window)

    await waitFor(() => expect(onChange).toHaveBeenCalledWith('#FFFFFF'))
    await waitFor(() =>
      expect(commands.rememberColor).toHaveBeenCalledExactlyOnceWith([255, 255, 255, 255]),
    )
  })

  it('commits an unfinished black drag when the picker closes', async () => {
    const onChange = vi.fn()
    await unfinishedBlackDrag(onChange)

    fireEvent.click(screen.getByRole('button', { name: 'Brush color' }))

    await waitFor(() => expect(onChange).toHaveBeenCalledExactlyOnceWith('#000000'))
    await waitFor(() => expect(commands.rememberColor).toHaveBeenCalledWith([0, 0, 0, 255]))
  })

  it('discards an unfinished drag on Escape', async () => {
    const onChange = vi.fn()
    const picker = await unfinishedBlackDrag(onChange)

    fireEvent.keyDown(picker, { key: 'Escape' })

    await waitFor(() => expect(screen.queryByRole('slider', { name: 'Color' })).toBeNull())
    expect(onChange).not.toHaveBeenCalled()
    expect(commands.rememberColor).not.toHaveBeenCalled()
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
    expect(commands.rememberColor).not.toHaveBeenCalled()
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

  it('loads app history in another color well and applies a swatch only on selection', async () => {
    const color: [number, number, number, number] = [18, 52, 86, 255]
    vi.mocked(commands.getRecentColors).mockResolvedValue([color])
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<ColorWell label='Text fill' value='#111111' onChange={onChange} />)

    await user.click(screen.getByRole('button', { name: 'Text fill' }))
    const swatch = await screen.findByRole('button', { name: /#123456/ })
    swatch.focus()
    fireEvent.mouseOver(swatch)
    expect(onChange).not.toHaveBeenCalled()
    expect(commands.rememberColor).not.toHaveBeenCalled()

    await user.click(swatch)
    expect(onChange).toHaveBeenCalledExactlyOnceWith('#123456')
    await waitFor(() => expect(commands.rememberColor).toHaveBeenCalledExactlyOnceWith(color))
  })

  it('keeps a late history load from replacing a newly applied color', async () => {
    const color: [number, number, number, number] = [171, 205, 239, 255]
    let resolveLoad!: (colors: (typeof color)[]) => void
    vi.mocked(commands.getRecentColors).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveLoad = resolve
        }),
    )
    render(<ColorWell value='#111111' onChange={vi.fn()} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Brush color' }))
    await waitFor(() => expect(commands.getRecentColors).toHaveBeenCalled())

    fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
      target: { value: '#ABCDEF' },
    })
    await waitFor(() => expect(queryClient.getQueryData(recentColorsKey)).toEqual([color]))
    await act(async () => {
      resolveLoad([[0, 0, 0, 255]])
    })
    expect(queryClient.getQueryData(recentColorsKey)).toEqual([color])
  })

  it('persists rapid applications in order across color wells', async () => {
    let resolveFirst!: (colors: [number, number, number, number][]) => void
    vi.mocked(commands.rememberColor).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveFirst = resolve
        }),
    )
    const user = userEvent.setup()
    render(
      <>
        <ColorWell label='Fill' value='#111111' onChange={vi.fn()} />
        <ColorWell label='Outline' value='#222222' onChange={vi.fn()} />
      </>,
    )
    await user.click(screen.getByRole('button', { name: 'Fill' }))
    fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
      target: { value: '#123456' },
    })
    await waitFor(() => expect(commands.rememberColor).toHaveBeenCalledTimes(1))
    await user.click(screen.getByRole('button', { name: 'Fill' }))
    await user.click(screen.getByRole('button', { name: 'Outline' }))
    fireEvent.change(screen.getByRole('textbox', { name: 'Hex color code' }), {
      target: { value: '#ABCDEF' },
    })
    expect(commands.rememberColor).toHaveBeenCalledTimes(1)

    await act(async () => {
      resolveFirst([[18, 52, 86, 255]])
    })
    await waitFor(() => expect(commands.rememberColor).toHaveBeenCalledTimes(2))
    expect(commands.rememberColor).toHaveBeenNthCalledWith(2, [171, 205, 239, 255])
    await waitFor(() =>
      expect(queryClient.getQueryData(recentColorsKey)).toEqual([[171, 205, 239, 255]]),
    )
  })
})

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
  // Deliberately omit mouseup to reproduce the lost drag-end event.
  fireEvent.mouseDown(picker, { buttons: 1, clientX: 0, clientY: 160 })
  await waitFor(() =>
    expect(picker).toHaveAttribute('aria-valuetext', 'Saturation 0%, Brightness 0%'),
  )
  expect(onChange).not.toHaveBeenCalled()
  expect(commands.rememberColor).not.toHaveBeenCalled()
  return picker
}

function CompleteSample() {
  const sampling = useColorSampling()
  return (
    <button type='button' onClick={() => sampling?.complete('#123456')}>
      Complete sample
    </button>
  )
}
