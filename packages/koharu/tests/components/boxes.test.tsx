import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { BoxActions } from '@/components/editor/BoxActions'
import { BoxJoinPreferences } from '@/components/preferences/BoxJoinPreferences'
import { pageKey, pagesKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
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
  vi.spyOn(commands, 'getBoxActions').mockResolvedValue({ can_split: true, bubble_boxes: 2 })
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pageKey, page)
  queryClient.setQueryData(pagesKey, [])
})

function mount(children: React.ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>)
}

describe('bubble box controls', () => {
  it('saves the independent join setting and retains its value when a save fails', async () => {
    const user = userEvent.setup()
    const save = vi
      .spyOn(commands, 'saveBoxSettings')
      .mockImplementation(async (settings) => settings)
    mount(<BoxJoinPreferences />)
    const control = await screen.findByRole('switch')
    await waitFor(() => expect(control).toBeEnabled())
    await user.click(control)
    await waitFor(() => expect(control).not.toBeChecked())
    expect(save).toHaveBeenCalledWith({ join_in_bubble: false })
    save.mockRejectedValueOnce(new Error('Save failed'))
    await user.click(control)
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(control).toBeEnabled())
    expect(control).not.toBeChecked()
  })

  it('selects the surviving box after joining and reruns a split through its command', async () => {
    const user = userEvent.setup()
    const join = vi.spyOn(commands, 'joinBubbleBoxes').mockResolvedValue('first')
    const split = vi.spyOn(commands, 'splitBox').mockResolvedValue(['box', 'new'])
    mount(<BoxActions layer='box' />)
    await user.click(await screen.findByRole('button', { name: 'Join with bubble' }))
    await waitFor(() => expect(useKoharuStore.getState().selectedLayers).toEqual(['first']))
    expect(join).toHaveBeenCalledWith('box')
    await user.click(await screen.findByRole('button', { name: 'Split' }))
    await waitFor(() => expect(useKoharuStore.getState().selectedLayers).toEqual(['box', 'new']))
    expect(split).toHaveBeenCalledWith('box')
  })

  it('hides Join when there is a single box and hides Split when there is no source region', async () => {
    vi.spyOn(commands, 'getBoxActions').mockResolvedValue({ can_split: true, bubble_boxes: 1 })
    const view = mount(<BoxActions layer='single' />)
    expect(await screen.findByRole('button', { name: 'Split' })).toBeEnabled()
    expect(screen.queryByRole('button', { name: 'Join with bubble' })).not.toBeInTheDocument()
    vi.spyOn(commands, 'getBoxActions').mockResolvedValue({ can_split: false, bubble_boxes: 0 })
    view.rerender(
      <QueryClientProvider client={queryClient}>
        <BoxActions layer='manual' />
      </QueryClientProvider>,
    )
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Split' })).not.toBeInTheDocument(),
    )
  })
})
