import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { SpeakerLine } from '@/components/editor/SpeakerLine'
import { TextLayerKind } from '@/components/redesign/TextLayerKind'
import { defaultTypography } from '@/components/redesign/typography'
import { projectKey, queryClient } from '@/lib/queries'
import translation from '@/public/locales/en-US/translation.json'
import { commands, type Layer, type LineSpeaker } from '@koharu/bridge/protocol'

const layer: Extract<Layer, { type: 'text' }> = {
  id: 'line',
  type: 'text',
  parent: 'page',
  angle_degrees: 0,
  layout: 'paragraph',
  automatic_region: null,
  visibility: { visible: true, opacity: 1 },
  geometry: { points: [] },
  content: { id: 'content', source: null, translation: null, role: null, source_region: null },
  typography: defaultTypography,
}

function setup(initial: LineSpeaker | null = null) {
  let speaker = initial
  const project = {
    name: 'chapter',
    revision: 1,
    active_page: null,
    can_undo: false,
    can_redo: false,
  }
  queryClient.setQueryData(projectKey, project)
  vi.spyOn(commands, 'getProject').mockImplementation(async () => project)
  vi.mocked(commands.getEffectiveCategory).mockResolvedValue({ category: 'shout', reason: 'model' })
  vi.mocked(commands.getChapterContext).mockImplementation(async () => ({
    context: { lines: { line: { speaker } }, series: 'series' },
    series: {
      id: 'series',
      name: 'Series',
      characters: [
        { id: 'mika', name: 'Mika', notes: 'Uses feminine wording.' },
        { id: 'ren', name: 'Ren', notes: '' },
      ],
    },
    supports_tasks: true,
  }))
  const save = vi.spyOn(commands, 'setBoxSpeaker').mockImplementation(async (_layer, value) => {
    speaker = { value, source: 'user' }
    project.revision += 1
    project.can_undo = true
    return speaker
  })
  const view = render(
    <QueryClientProvider client={queryClient}>
      <div data-testid='kind'>
        <TextLayerKind layer={layer} fallback='fallback' />
      </div>
      <SpeakerLine layer={layer.id} />
    </QueryClientProvider>,
  )
  return { save, view }
}

describe('box speakers', () => {
  it('sets an explicit None on an unlabeled box and accepts keyboard selection', async () => {
    const { save } = setup()
    const user = userEvent.setup()
    const picker = screen.getByRole('combobox', { name: translation.boxSpeaker.label })
    await waitFor(() => expect(picker).toBeEnabled())
    await user.click(picker)
    await user.click(await screen.findByRole('option', { name: translation.boxSpeaker.none }))
    await waitFor(() => expect(save).toHaveBeenLastCalledWith('line', null))
    await waitFor(() => expect(picker).toBeEnabled())
    await user.click(picker)
    const option = await screen.findByRole('option', { name: 'Ren' })
    option.focus()
    await user.keyboard('{Enter}')
    await waitFor(() => expect(save).toHaveBeenLastCalledWith('line', 'ren'))
    await waitFor(() => expect(picker).toHaveTextContent('Ren'))
  })

  it('commits only selection and refreshes the row and undo state, including explicit None', async () => {
    const { save } = setup({ value: 'mika', source: 'model' })
    const user = userEvent.setup()
    const picker = screen.getByRole('combobox', { name: translation.boxSpeaker.label })
    await waitFor(() => expect(picker).toBeEnabled())
    expect(picker).toHaveTextContent('Mika')
    expect(screen.getByTestId('kind')).toHaveTextContent('Mika')
    await user.click(picker)
    await user.hover(await screen.findByRole('option', { name: 'Ren' }))
    await user.keyboard('{Escape}')
    expect(save).not.toHaveBeenCalled()
    await user.click(picker)
    await user.click(await screen.findByRole('option', { name: 'Ren' }))
    await waitFor(() => expect(save).toHaveBeenLastCalledWith('line', 'ren'))
    await waitFor(() => expect(screen.getByTestId('kind')).toHaveTextContent('Ren'))
    expect(queryClient.getQueryData(projectKey)).toMatchObject({ can_undo: true })
    await waitFor(() => expect(picker).toBeEnabled())
    await user.click(picker)
    await user.click(await screen.findByRole('option', { name: translation.boxSpeaker.none }))
    await waitFor(() => expect(save).toHaveBeenLastCalledWith('line', null))
    await waitFor(() => expect(screen.getByTestId('kind')).not.toHaveTextContent('Ren'))
  })

  it('keeps the saved speaker after a failed edit and lets selection retry', async () => {
    const { save } = setup({ value: 'mika', source: 'user' })
    save.mockRejectedValueOnce(new Error('Disk full'))
    const user = userEvent.setup()
    const picker = screen.getByRole('combobox', { name: translation.boxSpeaker.label })
    await waitFor(() => expect(picker).toBeEnabled())
    await user.click(picker)
    await user.click(await screen.findByRole('option', { name: 'Ren' }))
    await screen.findByRole('alert')
    expect(picker).toHaveTextContent('Mika')
    await user.click(picker)
    await user.click(await screen.findByRole('option', { name: 'Ren' }))
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(picker).toHaveTextContent('Ren'))
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})
