import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { LibraryPreferences } from '@/components/preferences/LibraryPreferences'
import { queryClient } from '@/lib/queries'
import { commands } from '@koharu/bridge/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

function view(children: React.ReactNode) {
  return render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>{children}</TooltipProvider>
    </QueryClientProvider>,
  )
}

describe('file workflows', () => {
  it('rejects unknown export tokens in Settings without saving', async () => {
    const user = userEvent.setup()
    const save = vi.mocked(commands.saveLibrarySettings)
    view(<LibraryPreferences />)
    const input = screen.getByRole('textbox', { name: 'Export file names' })
    await waitFor(() => expect(input).toBeEnabled())
    await user.clear(input)
    await user.type(input, '{{unknown}')
    await user.click(screen.getByRole('button', { name: 'Save' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Unknown token {unknown}')
    expect(save).not.toHaveBeenCalled()
  })
})
