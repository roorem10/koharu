import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { ToolBar } from '@/components/editor/ToolBar'
import { pageKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import type { Page } from '@koharu/ipc/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

const page: Page = {
  id: 'page',
  size: { width: 800, height: 1200 },
  background: null,
  layers: [],
}

/** Koharu 0.95's tools in its rail, in order: the O tool folded into its text block tool, K. */
const koharuTools = [
  'Select',
  'Direct Selection',
  'Rectangular Marquee',
  'Lasso',
  'Text',
  'Text block',
  'Brush',
  'Eraser',
  'Color picker',
  'Remove',
  'Pan',
]

describe("Booharu's K tool in the tool rail", () => {
  it("holds Koharu's tools and no O tool, and shows K's modes under its name and key", async () => {
    queryClient.setQueryData(pageKey, page)
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <TooltipProvider delay={0}>
          <ToolBar />
        </TooltipProvider>
      </QueryClientProvider>,
    )

    // The tools come first, then the colour well and the brush size.
    const names = screen.getAllByRole('button').map((button) => button.getAttribute('aria-label'))
    expect(names.slice(0, koharuTools.length)).toEqual(koharuTools)
    expect(names).not.toContain('OCR box')

    const tool = screen.getByRole('button', { name: 'Text block' })
    await user.hover(tool)
    const hint = await screen.findByText('Alt: read only. Shift: read, translate and clean.')
    expect(hint.parentElement).toHaveTextContent(/^Text blockK/)
    // app/booharu.css wraps the tooltip's row for the hint by this parent.
    expect(hint.parentElement).toHaveAttribute('data-slot', 'tooltip-content')

    await user.click(tool)
    expect(useKoharuStore.getState().tool).toBe('block')
    expect(tool).toHaveAttribute('data-active', 'true')
  })
})
