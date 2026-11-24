import { QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { ToolBar } from '@/components/editor/ToolBar'
import { useBrushSlots } from '@/lib/booharu/brush'
import { pageKey, queryClient } from '@/lib/queries'
import { useKoharuStore, type CanvasTool } from '@/lib/store'
import type { Page } from '@koharu/ipc/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

const page: Page = { id: 'page', size: { width: 800, height: 1200 }, background: null, layers: [] }

const removeNote =
  "Clean-up goes under the lettering. To fix a box's own clean-up, paint into it or clean it again."

/** The tool rail with `tool` picked and its brush size popover open, which the popover's options
 * sit in. */
async function openBrush(tool: CanvasTool) {
  const user = userEvent.setup()
  useKoharuStore.setState({ tool })
  render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <ToolBar />
      </TooltipProvider>
    </QueryClientProvider>,
  )
  await user.click(screen.getByRole('button', { name: 'Brush size: 48 pixels' }))
  return { user, popover: await screen.findByRole('dialog') }
}

/** The names of the number boxes the popover shows, by their labels, in order. */
function fields(popover: HTMLElement): string[] {
  return within(popover)
    .getAllByRole('textbox')
    .map((field) => field.getAttribute('aria-label') ?? labelOf(field))
}

/** The popover's sliders, whose thumbs Base UI makes range inputs. */
function sliders(popover: HTMLElement): HTMLInputElement[] {
  return [...popover.querySelectorAll<HTMLInputElement>('input[type="range"]')]
}

/** The text of the element `field` is labelled by. */
function labelOf(field: HTMLElement): string {
  const id = field.getAttribute('aria-labelledby')
  return (id && document.getElementById(id)?.textContent) || ''
}

beforeEach(() => {
  queryClient.setQueryData(pageKey, page)
})

afterEach(() => {
  cleanup()
  document.body.replaceChildren()
})

describe("Booharu's brush options in the brush size popover", () => {
  it("shows Draw's hardness and opacity under its size", async () => {
    const { popover } = await openBrush('draw')
    expect(fields(popover)).toEqual(['Brush size', 'Hardness', 'Opacity'])
    expect(sliders(popover)).toHaveLength(3)
    expect(within(popover).queryByText(removeNote)).toBeNull()
  })

  it("shows the Eraser's hardness alone", async () => {
    const { popover } = await openBrush('eraser')
    expect(fields(popover)).toEqual(['Brush size', 'Hardness'])
    expect(sliders(popover)).toHaveLength(2)
  })

  it("shows Remove's hardness, and where its clean-up goes", async () => {
    const { popover } = await openBrush('remove')
    expect(fields(popover)).toEqual(['Brush size', 'Hardness'])
    expect(within(popover).getByText(removeNote)).toBeInTheDocument()
  })

  it('shows nothing more for a tool without a brush', async () => {
    const { popover } = await openBrush('select')
    expect(fields(popover)).toEqual(['Brush size'])
  })

  it('shows each share in whole percent, the number alone in its box and its % after it', async () => {
    useBrushSlots.setState({ draw: { diameter: 48, hardness: 0.35, opacity: 1 } })
    const { popover } = await openBrush('draw')
    // "100%" would be wider than the box, which shows "100" whole.
    const hardness = within(popover).getByRole('textbox', { name: 'Hardness' })
    expect(hardness).toHaveValue('35')
    expect(hardness).toHaveAccessibleDescription('%')
    const opacity = within(popover).getByRole('textbox', { name: 'Opacity' })
    expect(opacity).toHaveValue('100')
    expect(opacity).toHaveAccessibleDescription('%')
  })

  it("writes Draw's hardness and opacity, as percentages, to Draw's brush alone", async () => {
    const { user, popover } = await openBrush('draw')
    const hardness = within(popover).getByRole('textbox', { name: 'Hardness' })
    await user.clear(hardness)
    await user.type(hardness, '35')
    expect(useBrushSlots.getState().draw.hardness).toBe(0.35)

    await user.click(within(popover).getByRole('button', { name: 'Decrease opacity' }))
    expect(useBrushSlots.getState().draw.opacity).toBe(0.99)
    expect(useBrushSlots.getState().eraser).toEqual({ diameter: 48, hardness: 1 })
    expect(useBrushSlots.getState().remove).toEqual({ diameter: 48, hardness: 1 })
  })

  it('keeps opacity above 0 and hardness from 0 to 100%', async () => {
    const { user, popover } = await openBrush('draw')
    const opacity = within(popover).getByRole('textbox', { name: 'Opacity' })
    await user.clear(opacity)
    await user.type(opacity, '0')
    expect(useBrushSlots.getState().draw.opacity).toBe(0.01)

    const hardness = within(popover).getByRole('textbox', { name: 'Hardness' })
    await user.clear(hardness)
    await user.type(hardness, '0')
    expect(useBrushSlots.getState().draw.hardness).toBe(0)
  })

  it("writes the Eraser's and Remove's hardness, from the slider too, to their own brushes", async () => {
    const { popover } = await openBrush('eraser')
    const [, slider] = sliders(popover)
    // Named as the size slider is, on the slider's group.
    expect(slider?.closest('[data-slot="slider"]')).toHaveAccessibleName('Hardness')
    act(() => slider?.focus())
    fireEvent.keyDown(slider as HTMLElement, { key: 'ArrowLeft' })
    expect(useBrushSlots.getState().eraser.hardness).toBe(0.99)
    cleanup()

    const remove = await openBrush('remove')
    await remove.user.click(
      within(remove.popover).getByRole('button', { name: 'Decrease hardness' }),
    )
    expect(useBrushSlots.getState().remove.hardness).toBe(0.99)
    expect(useBrushSlots.getState().draw).toEqual({ diameter: 48, hardness: 1, opacity: 1 })
  })
})
