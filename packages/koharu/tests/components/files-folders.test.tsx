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
  it('changes and resets a model folder without changing the active folder', async () => {
    const user = userEvent.setup()
    vi.mocked(commands.chooseLibraryFolder).mockResolvedValue({
      export_name_pattern: '{index}_{page}',
      projects_folder: null,
      models_folder: 'D:\\AI Models',
    })
    view(<LibraryPreferences />)
    const buttons = screen.getAllByRole('button', { name: 'Change…' })
    await waitFor(() => expect(buttons[1]).toBeEnabled())
    await user.click(buttons[1]!)
    await waitFor(() =>
      expect(commands.chooseLibraryFolder).toHaveBeenCalledExactlyOnceWith('models'),
    )
    expect(await screen.findByText('Next launch: D:\\AI Models')).toBeVisible()
    expect(screen.getByText('C:\\Models', { exact: true })).toBeVisible()
    await user.click(screen.getAllByRole('button', { name: 'Use default' })[1]!)
    await waitFor(() =>
      expect(commands.resetLibraryFolder).toHaveBeenCalledExactlyOnceWith('models'),
    )
    await waitFor(() =>
      expect(screen.queryByText('Next launch: D:\\AI Models')).not.toBeInTheDocument(),
    )
  })
})
