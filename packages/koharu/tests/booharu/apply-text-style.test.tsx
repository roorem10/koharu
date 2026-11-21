import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { TypesettingPreferences } from '@/components/preferences/TypesettingPreferences'
import { type ProjectView, projectKey, queryClient } from '@/lib/queries'
import { commands, type StyleCounts } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { settingsDesktop } from './settings'

const PROJECT = { kind: 'project' } as const
const BUTTON = 'Apply to the open project'
const INCLUDE = 'Also restyle boxes I changed by hand'

/** What the desktop counts in the open project, without and with the boxes changed by hand. */
const counted: Record<'kept' | 'included', StyleCounts> = {
  kept: { changed: 4, kept_hand_edited: 3, untranslated: 2 },
  included: { changed: 7, kept_hand_edited: 1, untranslated: 2 },
}

/** A project open in the editor. */
function openProject() {
  const project: ProjectView = {
    name: 'Demo',
    page: 'page',
    pages: [
      { id: 'page', size: { width: 800, height: 1200 }, background: null, texts: 0, translated: 0 },
    ],
    history: { steps: [], current: 0 },
    revisions: {},
  }
  queryClient.setQueryData(projectKey, project)
}

/** The desktop, counting and applying the default text style as `counted` says, and the outline
 * width the settings file held for each count and each apply in `widths`. */
function desktop() {
  const { file } = settingsDesktop()
  const widths: Array<number | undefined> = []
  const answer = async (_: unknown, include: boolean) => {
    widths.push(file.preferences['booharu-lettering'].default?.stroke_width)
    return include ? counted.included : counted.kept
  }
  const count = vi.spyOn(commands, 'booharuCountTextStyleTargets').mockImplementation(answer)
  const apply = vi.spyOn(commands, 'booharuApplyTextStyles').mockImplementation(answer)
  return { count, apply, widths }
}

/** Settings > Typesetting, where Apply is the default text style's last row. */
function renderTypesetting() {
  render(
    <QueryClientProvider client={queryClient}>
      <TypesettingPreferences
        value={{ font_families: ['CCWildWords'] }}
        onChange={() => undefined}
      />
    </QueryClientProvider>,
  )
}

/** The default text style's Apply, its last row: the Lettering categories card has one too. */
async function applyButton() {
  const section = within(await screen.findByRole('region', { name: 'Default text style' }))
  return section.findByRole('button', { name: BUTTON })
}

/** The confirmation, once it shows. */
async function confirmation() {
  return within(await screen.findByRole('alertdialog'))
}

describe('Booharu Apply to the open project', () => {
  it('is off while no project is open', async () => {
    desktop()
    renderTypesetting()

    const button = await applyButton()
    await waitFor(() => expect(queryClient.getQueryState(projectKey)?.status).toBe('success'))
    expect(button).toBeDisabled()
  })

  it('counts first, counts again with the boxes changed by hand, then applies and says what it did', async () => {
    const { count, apply } = desktop()
    const shown = vi.spyOn(toast, 'add')
    openProject()
    const user = userEvent.setup()
    renderTypesetting()

    await user.click(await applyButton())
    let dialog = await confirmation()
    expect(count).toHaveBeenCalledWith(PROJECT, false)
    expect(dialog.getByText('4 boxes will be restyled.')).toBeInTheDocument()
    expect(dialog.getByText('3 boxes you changed by hand or locked are kept.')).toBeInTheDocument()
    expect(dialog.getByText('2 boxes have no translation yet.')).toBeInTheDocument()
    const include = dialog.getByRole('checkbox', { name: INCLUDE })
    expect(include).not.toBeChecked()

    await user.click(include)
    await waitFor(() => expect(dialog.getByText('7 boxes will be restyled.')).toBeInTheDocument())
    expect(count).toHaveBeenLastCalledWith(PROJECT, true)
    expect(include).toBeChecked()
    // With those included, only locked boxes are kept.
    expect(dialog.getByText('1 locked box is kept.')).toBeInTheDocument()

    await user.click(dialog.getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull())
    expect(apply).toHaveBeenCalledOnce()
    expect(apply).toHaveBeenCalledWith(PROJECT, true)
    expect(shown).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'success',
        title: 'Restyled 7 boxes',
        description: '1 locked box is kept.',
      }),
    )

    // Asked again, it starts from the boxes changed by hand kept.
    await user.click(await applyButton())
    dialog = await confirmation()
    expect(count).toHaveBeenLastCalledWith(PROJECT, false)
    expect(dialog.getByRole('checkbox', { name: INCLUDE })).not.toBeChecked()
    expect(dialog.getByText('4 boxes will be restyled.')).toBeInTheDocument()
  })

  it('shows one box in the singular, and offers no Apply when nothing would change', async () => {
    const { count, apply } = desktop()
    count.mockResolvedValueOnce({ changed: 1, kept_hand_edited: 1, untranslated: 1 })
    count.mockResolvedValueOnce({ changed: 0, kept_hand_edited: 0, untranslated: 5 })
    openProject()
    const user = userEvent.setup()
    renderTypesetting()

    await user.click(await applyButton())
    let dialog = await confirmation()
    expect(dialog.getByText('1 box will be restyled.')).toBeInTheDocument()
    expect(dialog.getByText('1 box you changed by hand or locked is kept.')).toBeInTheDocument()
    expect(dialog.getByText('1 box has no translation yet.')).toBeInTheDocument()
    await user.click(dialog.getByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull())

    await user.click(await applyButton())
    dialog = await confirmation()
    expect(dialog.getByText('0 boxes will be restyled.')).toBeInTheDocument()
    expect(dialog.queryByText(/kept\./)).toBeNull()
    expect(dialog.getByText('5 boxes have no translation yet.')).toBeInTheDocument()
    expect(dialog.getByRole('button', { name: 'Apply' })).toBeDisabled()
    expect(apply).not.toHaveBeenCalled()
  })

  it('counts and applies the look the section shows, a change made just before saved first', async () => {
    const { count, apply, widths } = desktop()
    openProject()
    const user = userEvent.setup()
    renderTypesetting()

    const region = within(await screen.findByRole('region', { name: 'Default text style' }))
    await waitFor(() =>
      expect(region.getByRole('button', { name: 'Increase outline width' })).toBeEnabled(),
    )
    // Asked for before the change's own save would come.
    await user.click(region.getByRole('button', { name: 'Increase outline width' }))
    await user.click(await applyButton())
    const dialog = await confirmation()
    await user.click(dialog.getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(apply).toHaveBeenCalledOnce())

    expect(count).toHaveBeenCalledOnce()
    expect(widths).toEqual([2, 2])
  })

  it('leaves the box and the numbers as they were when counting again fails', async () => {
    const { count, apply } = desktop()
    const shown = vi.spyOn(toast, 'add')
    openProject()
    const user = userEvent.setup()
    renderTypesetting()

    await user.click(await applyButton())
    const dialog = await confirmation()
    count.mockRejectedValueOnce('The project is busy')
    await user.click(dialog.getByRole('checkbox', { name: INCLUDE }))

    // As every failed command shows its error.
    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'error', description: 'The project is busy' }),
      ),
    )
    await waitFor(() => expect(dialog.getByRole('checkbox', { name: INCLUDE })).not.toBeChecked())
    expect(dialog.getByText('4 boxes will be restyled.')).toBeInTheDocument()

    await user.click(dialog.getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(apply).toHaveBeenCalledWith(PROJECT, false))
  })
})
