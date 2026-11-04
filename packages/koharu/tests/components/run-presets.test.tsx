import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'

import { InferenceControl } from '@/components/editor/InferenceControl'
import { RunPreferences } from '@/components/preferences/RunPreferences'
import { projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/bridge/protocol'
const project = {
  name: 'chapter',
  revision: 1,
  active_page: 'page',
  can_undo: false,
  can_redo: false,
}
function show(children: React.ReactNode) {
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  queryClient.setQueryData(projectKey, project)
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>)
}
it('runs a preset on the current scope without changing the selected stages', async () => {
  const run = vi.fn()
  useKoharuStore.setState({
    processingScope: 'selected-pages',
    selectedPages: ['p2', 'p3'],
    processingStages: ['translation'],
  })
  vi.spyOn(commands, 'getTranslationModels').mockResolvedValue([])
  show(<InferenceControl onRun={run} disabled={false} />)
  const user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: 'Processing settings' }))
  await user.click(await screen.findByRole('button', { name: 'Detect + OCR' }))
  expect(run).toHaveBeenCalledWith('selected-pages', ['detection', 'ocr'])
  expect(useKoharuStore.getState().processingStages).toEqual(['translation'])
})

it('keeps all run settings while adding a named preset', async () => {
  const save = vi
    .spyOn(commands, 'saveRunSettings')
    .mockImplementation(async (settings) => settings)
  show(<RunPreferences />)
  const user = userEvent.setup()
  await user.type(await screen.findByRole('textbox', { name: 'New preset name' }), 'My workflow')
  await user.click(screen.getByRole('button', { name: 'Add' }))
  await waitFor(() => expect(save).toHaveBeenCalledOnce())
  expect(save.mock.lastCall?.[0]).toMatchObject({
    retries: 2,
    order: 'overlapped',
    finish_sound: true,
  })
  expect(save.mock.lastCall?.[0].presets?.at(-1)).toEqual({
    name: 'My workflow',
    stages: ['detection', 'ocr', 'translation', 'inpainting'],
  })
})

it('cancels a rename with Escape and preserves the next preset when deleting a row', async () => {
  const save = vi
    .spyOn(commands, 'saveRunSettings')
    .mockImplementation(async (settings) => settings)
  show(<RunPreferences />)
  const user = userEvent.setup()
  const input = (await screen.findAllByRole('textbox', { name: 'Preset name' }))[0]
  await user.clear(input)
  await user.type(input, 'Canceled name')
  await user.keyboard('{Escape}')
  expect(save).not.toHaveBeenCalled()
  expect(input).toHaveValue('Detect + OCR')
  await user.clear(input)
  await user.type(input, 'Renamed preset')
  await user.keyboard('{Enter}')
  await waitFor(() => expect(save.mock.lastCall?.[0].presets?.[0].name).toBe('Renamed preset'))
  await waitFor(() => expect(screen.getAllByRole('button', { name: 'Delete' })[0]).toBeEnabled())
  await user.click(screen.getAllByRole('button', { name: 'Delete' })[0])
  await waitFor(() =>
    expect(save.mock.lastCall?.[0].presets).toEqual([
      { name: 'Translate + clean up', stages: ['translation', 'inpainting'] },
    ]),
  )
  await waitFor(() =>
    expect(
      screen.getByRole('checkbox', { name: 'Translate + clean up: Translation' }),
    ).toBeEnabled(),
  )
  await user.click(screen.getByRole('checkbox', { name: 'Translate + clean up: Translation' }))
  await waitFor(() => expect(save.mock.lastCall?.[0].presets?.[0].stages).toEqual(['inpainting']))
  await waitFor(() =>
    expect(
      screen.getByRole('checkbox', { name: 'Translate + clean up: Inpainting' }),
    ).toHaveAttribute('aria-disabled', 'true'),
  )
})
