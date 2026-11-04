import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { TitleBar } from '@/components/app/TitleBar'
import { RawPrepPreferences } from '@/components/preferences/RawPrepPreferences'
import { pageKey, pagesKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/bridge/protocol'
import { toast } from '@koharu/ui/components/toast'

const options = { levels: true, clip_percent: 0.5, whiten: false, threshold: 235 }
const project = {
  name: 'Raw prep',
  revision: 1,
  active_page: 'page-2',
  can_undo: false,
  can_redo: false,
}
const pages = ['page-1', 'page-2'].map((id) => ({
  id,
  label: `${id}.png`,
  size: { width: 100, height: 200 },
  source_asset: id,
  layer_count: 1,
}))

beforeEach(() => {
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  vi.spyOn(commands, 'getPages').mockResolvedValue(pages)
  vi.spyOn(commands, 'getPage').mockResolvedValue({ ...pages[1], layers: [], regions: [] })
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pagesKey, pages)
  queryClient.setQueryData(pageKey, { ...pages[1], layers: [], regions: [] })
  useKoharuStore.setState({ selectedPages: ['page-2'] })
})

function mount(children: React.ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>)
}

describe('page levels', () => {
  it.each(['selected', 'all'] as const)(
    'keeps the dialog after the menu closes and applies the chosen options to %s pages',
    async (scope) => {
      const user = userEvent.setup()
      mount(<TitleBar />)
      await user.click(screen.getByRole('menuitem', { name: 'Process' }))
      await user.click(await screen.findByRole('menuitem', { name: 'Fix page levels…' }))
      expect(await screen.findByRole('dialog')).toBeInTheDocument()
      await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument())
      await waitFor(() => expect(commands.previewPrep).toHaveBeenCalledWith('page-2', options))
      expect(commands.preparePages).not.toHaveBeenCalled()
      fireEvent.change(screen.getByRole('spinbutton', { name: 'Clip at each end (%)' }), {
        target: { value: '1.2' },
      })
      await user.click(screen.getByRole('checkbox', { name: 'Whiten paper' }))
      fireEvent.change(screen.getByRole('spinbutton', { name: 'Whitening threshold (0–255)' }), {
        target: { value: '240' },
      })
      if (scope === 'all') {
        await user.click(screen.getByRole('combobox', { name: 'Pages to fix' }))
        await user.click(await screen.findByRole('option', { name: 'All pages' }))
      }
      await user.click(screen.getByRole('button', { name: 'Apply' }))
      await waitFor(() =>
        expect(commands.preparePages).toHaveBeenCalledWith(
          scope === 'all' ? ['page-1', 'page-2'] : ['page-2'],
          { levels: true, clip_percent: 1.2, whiten: true, threshold: 240 },
        ),
      )
      await screen.findByRole('button', { name: 'Done' })
    },
  )

  it('reports skipped pages without applying the preview or changing the scope', async () => {
    const user = userEvent.setup()
    vi.mocked(commands.preparePages).mockResolvedValue({
      changed: 0,
      skipped: [{ page: 'page-2', label: 'page-2.png', reason: 'cleanup' }],
    })
    mount(<TitleBar />)
    await user.click(screen.getByRole('menuitem', { name: 'Process' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Fix page levels…' }))
    await screen.findByRole('button', { name: 'Apply' })
    await waitFor(() => expect(screen.getByRole('button', { name: 'Apply' })).toBeEnabled())
    await user.click(screen.getByRole('button', { name: 'Apply' }))
    expect(
      await screen.findByText('page-2.png: a Cleanup layer already exists'),
    ).toBeInTheDocument()
    expect(screen.getByText('Fixed levels on 0 pages.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Done' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  })

  it('enables Restore only for selected pages with an original and still reports its result after the menu closes', async () => {
    const user = userEvent.setup()
    const notify = vi.spyOn(toast, 'add')
    vi.mocked(commands.getPrepPageStatus).mockImplementation(async (selected) =>
      selected.map((page) => ({ page, has_original: page === 'page-1', has_cleanup: false })),
    )
    vi.mocked(commands.restoreOriginalPages).mockResolvedValue({ changed: 1, skipped: [] })
    mount(<TitleBar />)
    await user.click(screen.getByRole('menuitem', { name: 'Process' }))
    await waitFor(() => expect(commands.getPrepPageStatus).toHaveBeenCalledWith(['page-2']))
    expect(screen.getByRole('menuitem', { name: 'Restore original pages' })).toHaveAttribute(
      'aria-disabled',
      'true',
    )
    act(() => useKoharuStore.setState({ selectedPages: ['page-1'] }))
    await waitFor(() =>
      expect(screen.getByRole('menuitem', { name: 'Restore original pages' })).not.toHaveAttribute(
        'aria-disabled',
        'true',
      ),
    )
    await user.click(screen.getByRole('menuitem', { name: 'Restore original pages' }))
    await waitFor(() => expect(commands.restoreOriginalPages).toHaveBeenCalledWith(['page-1']))
    await waitFor(() =>
      expect(notify).toHaveBeenCalledWith(
        expect.objectContaining({ title: 'Restored 1 original page.' }),
      ),
    )
  })

  it('uses all pages when there is no selection and allows cancelling without changes', async () => {
    const user = userEvent.setup()
    useKoharuStore.setState({ selectedPages: [] })
    mount(<TitleBar />)
    await user.click(screen.getByRole('menuitem', { name: 'Process' }))
    await user.click(await screen.findByRole('menuitem', { name: 'Fix page levels…' }))
    await waitFor(() => expect(commands.previewPrep).toHaveBeenCalledWith('page-1', options))
    expect(screen.getByRole('combobox', { name: 'Pages to fix' })).toHaveTextContent('All pages')
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(commands.preparePages).not.toHaveBeenCalled()
  })

  it('saves import preparation separately from the pipeline configuration', async () => {
    const user = userEvent.setup()
    const pipeline = vi.spyOn(commands, 'savePreferences')
    mount(<RawPrepPreferences />)
    const toggle = await screen.findByRole('checkbox', { name: 'Fix levels on import' })
    expect(toggle).toHaveAttribute('aria-checked', 'false')
    await user.click(toggle)
    await waitFor(() =>
      expect(commands.saveRawPrepSettings).toHaveBeenCalledWith({ options, fix_on_import: true }),
    )
    expect(pipeline).not.toHaveBeenCalled()
  })
})
