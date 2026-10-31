import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { StartView } from '@/components/start/StartView'
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
  it('renames and duplicates from the start screen without opening the project', async () => {
    const user = userEvent.setup()
    vi.spyOn(commands, 'listProjects').mockResolvedValue([{ name: 'Chapter' }])
    const open = vi.spyOn(commands, 'openProject')
    view(<StartView />)
    await user.click(await screen.findByRole('button', { name: 'Rename Chapter' }))
    const name = screen.getByRole('textbox', { name: 'New project name' })
    await user.clear(name)
    await user.type(name, 'Chapter revised')
    await user.click(screen.getByRole('button', { name: 'Rename' }))
    await waitFor(() =>
      expect(commands.renameProject).toHaveBeenCalledExactlyOnceWith('Chapter', 'Chapter revised'),
    )
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    await user.click(screen.getByRole('button', { name: 'Duplicate Chapter' }))
    await waitFor(() =>
      expect(commands.duplicateProject).toHaveBeenCalledExactlyOnceWith('Chapter'),
    )
    expect(open).not.toHaveBeenCalled()
  })
})
