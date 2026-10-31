import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { useScriptFiles } from '@/components/files/ScriptImport'
import { queryClient } from '@/lib/queries'
import { commands, type ScriptPreview } from '@koharu/bridge/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

function view(children: React.ReactNode) {
  return render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>{children}</TooltipProvider>
    </QueryClientProvider>,
  )
}

function ScriptHarness() {
  const scripts = useScriptFiles()
  return (
    <>
      <button onClick={() => void scripts.importScript()}>Import</button>
      {scripts.dialog}
    </>
  )
}

describe('file workflows', () => {
  it('previews translations and applies all changes in a single native call', async () => {
    const user = userEvent.setup()
    const preview: ScriptPreview = {
      project: 'project',
      project_name: 'Chapter',
      revision: 1,
      unchanged: 4,
      not_found: 1,
      changes: [
        {
          layer: 'one',
          page: 1,
          page_label: 'page.png',
          box_number: 1,
          old_text: 'Old one',
          new_text: 'New one',
        },
        {
          layer: 'two',
          page: 2,
          page_label: 'next.png',
          box_number: 2,
          old_text: 'Old two',
          new_text: 'New\n\nparagraph',
        },
      ],
    }
    vi.mocked(commands.previewScriptImport).mockResolvedValue(preview)
    vi.spyOn(commands, 'getProject').mockResolvedValue(null)
    vi.spyOn(commands, 'getPages').mockResolvedValue([])
    vi.spyOn(commands, 'getPage').mockResolvedValue(null)
    view(<ScriptHarness />)
    await user.click(screen.getByRole('button', { name: 'Import' }))
    expect(await screen.findByText('2 changed · 4 unchanged · 1 IDs not found')).toBeVisible()
    expect(screen.getByText('Old one')).toBeVisible()
    expect(screen.getByText('New one')).toBeVisible()
    expect(commands.applyScriptImport).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(commands.applyScriptImport).toHaveBeenCalledExactlyOnceWith(preview))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  })
})
