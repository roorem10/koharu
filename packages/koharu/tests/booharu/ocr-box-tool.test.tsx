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

describe('Booharu O tool in the tool rail', () => {
  it('shows the O tool before Pan with its key and modes, and selects it', async () => {
    queryClient.setQueryData(pageKey, page)
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <TooltipProvider delay={0}>
          <ToolBar />
        </TooltipProvider>
      </QueryClientProvider>,
    )

    const names = screen.getAllByRole('button').map((button) => button.getAttribute('aria-label'))
    const index = names.indexOf('Read text in a box')
    expect(names.slice(index - 1, index + 2)).toEqual(['Remove', 'Read text in a box', 'Pan'])

    const tool = screen.getByRole('button', { name: 'Read text in a box' })
    await user.hover(tool)
    const hint = await screen.findByText('Alt: read only. Shift: read, translate and clean.')
    expect(hint.parentElement).toHaveTextContent(/^Read text in a boxO/)
    // app/booharu.css wraps the tooltip's row for the hint by this parent.
    expect(hint.parentElement).toHaveAttribute('data-slot', 'tooltip-content')

    await user.click(tool)
    expect(useKoharuStore.getState().tool).toBe('ocr_box')
    expect(tool).toHaveAttribute('data-active', 'true')
  })
})
