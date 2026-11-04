import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { TitleBar } from '@/components/app/TitleBar'
import { pageKey, pagesKey, projectKey, queryClient } from '@/lib/queries'
import { commands } from '@koharu/bridge/protocol'

const project = {
  name: 'Boxes',
  revision: 1,
  active_page: 'page',
  can_undo: false,
  can_redo: false,
}
const page = {
  id: 'page',
  label: 'Page',
  size: { width: 200, height: 200 },
  layers: [],
  regions: [],
}

beforeEach(() => {
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  vi.spyOn(commands, 'getPage').mockResolvedValue(page)
  vi.spyOn(commands, 'getPages').mockResolvedValue([])
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pageKey, page)
  queryClient.setQueryData(pagesKey, [])
})

function mount(children: React.ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>)
}

describe('page re-detection', () => {
  it('keeps the re-detection confirmation open after the Edit menu closes and runs only on confirm', async () => {
    const user = userEvent.setup()
    const detect = vi.spyOn(commands, 'redetectPage').mockResolvedValue('job')
    mount(<TitleBar />)
    await user.click(screen.getByRole('menuitem', { name: 'Edit' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Detect page again' }))
    const dialog = await screen.findByRole('alertdialog')
    expect(dialog).toHaveTextContent('OCR text and translations')
    expect(detect).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument())
    expect(detect).not.toHaveBeenCalled()
    await user.click(screen.getByRole('menuitem', { name: 'Edit' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Detect page again' }))
    await user.click(await screen.findByRole('button', { name: 'Detect page again' }))
    await waitFor(() => expect(detect).toHaveBeenCalledWith('page'))
  })
})
