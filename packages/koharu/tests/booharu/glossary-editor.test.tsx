import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { GlossaryDialog } from '@/components/booharu/context/GlossaryDialog'
import { PipelinePreferences } from '@/components/preferences/PipelinePreferences'
import { queryClient } from '@/lib/queries'

import { blueLock, contextDesktop, openChapter } from './context'

type User = ReturnType<typeof userEvent.setup>

/** The glossary editor, open on the series Blue Lock, once it shows it. */
async function editor() {
  render(
    <QueryClientProvider client={queryClient}>
      <GlossaryDialog initial={blueLock().id} onClose={() => undefined} />
    </QueryClientProvider>,
  )
  const dialog = await screen.findByRole('dialog', { name: 'Glossary' })
  await within(dialog).findByRole('textbox', { name: 'Series name' })
  return within(dialog)
}

/** The term table's row of the term `source`. */
async function termRow(dialog: ReturnType<typeof within>, source: string) {
  const cell = await dialog.findByRole('cell', { name: new RegExp(`^${source}`) })
  return within(cell.closest('tr')!)
}

/** Writes `text` in place of what `field` holds and moves on, which commits it. */
async function rewrite(user: User, field: HTMLElement, text: string) {
  await user.clear(field)
  if (text) await user.type(field, text)
  await user.tab()
}

describe('Booharu glossary editor', () => {
  it('opens from the Glossary row in Settings > Pipeline, on the chapter’s series', async () => {
    contextDesktop()
    openChapter()
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <PipelinePreferences value={{}} models={[]} onChange={() => undefined} />
      </QueryClientProvider>,
    )

    await user.click(await screen.findByRole('button', { name: 'Open full glossary' }))

    const dialog = within(await screen.findByRole('dialog', { name: 'Glossary' }))
    expect(await dialog.findByRole('textbox', { name: 'Series name' })).toHaveValue('Blue Lock')
    expect(dialog.getByRole('combobox', { name: 'Series' })).toHaveTextContent('Blue Lock')
  })

  it('saves a term through the term form, in place of one with its source', async () => {
    const { files, saves } = contextDesktop()
    const user = userEvent.setup()
    const dialog = await editor()

    await user.click(dialog.getByRole('button', { name: 'Add term' }))
    let form = within(await screen.findByRole('dialog', { name: 'Glossary entry' }))
    const save = form.getByRole('button', { name: 'Save' })
    expect(save).toBeDisabled()
    await user.type(form.getByRole('textbox', { name: 'Source' }), 'えご')
    await user.type(form.getByRole('textbox', { name: 'Translation' }), 'Ego')
    expect(form.queryByText(/^Saving replaces/)).toBeNull()
    await user.clear(form.getByRole('textbox', { name: 'Source' }))
    await user.type(form.getByRole('textbox', { name: 'Source' }), ' エゴ')
    expect(form.getByText('Saving replaces エゴ → ego.')).toBeInTheDocument()
    await user.click(form.getByRole('combobox', { name: 'Kind' }))
    await user.click(await screen.findByRole('option', { name: 'Name' }))
    await user.type(form.getByRole('textbox', { name: 'Notes' }), 'Capitalised.')
    await user.click(form.getByRole('checkbox', { name: 'Lock this translation' }))
    await user.click(save)

    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]!.glossary).toEqual([
      blueLock().glossary[0],
      { id: 't2', source: 'エゴ', target: 'Ego', note: 'Capitalised.', kind: 'name', locked: true },
    ])
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Glossary entry' })).toBeNull())

    // Editing a term keeps the rest of the series as its file holds it, a term the agent added
    // meanwhile included.
    files.series.get(blueLock().id)!.glossary!.push({
      id: 'agent',
      source: '凛',
      target: 'Rin',
      note: '',
      kind: 'name',
      locked: false,
    })
    await user.click((await termRow(dialog, '潔')).getByRole('button', { name: 'Edit' }))
    form = within(await screen.findByRole('dialog', { name: 'Glossary entry' }))
    expect(form.getByRole('checkbox', { name: 'Lock this translation' })).toBeChecked()
    await rewrite(user, form.getByRole('textbox', { name: 'Translation' }), 'Yoichi Isagi')
    await user.click(form.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(saves[1]!.glossary!.map((entry) => `${entry.source}=${entry.target}`)).toEqual([
      '潔=Yoichi Isagi',
      'エゴ=Ego',
      '凛=Rin',
    ])

    await user.click((await termRow(dialog, 'エゴ')).getByRole('button', { name: 'Delete' }))
    await waitFor(() => expect(saves).toHaveLength(3))
    expect(saves[2]!.glossary!.map((entry) => entry.source)).toEqual(['潔', '凛'])
    await waitFor(() => expect(dialog.queryByRole('cell', { name: /^エゴ/ })).toBeNull())
  })

  it('renames the series as its name is committed, never to a blank one', async () => {
    const { spies } = contextDesktop()
    const user = userEvent.setup()
    const dialog = await editor()
    const name = dialog.getByRole('textbox', { name: 'Series name' })

    await rewrite(user, name, '')
    expect(name).toHaveValue('Blue Lock')
    expect(spies.rename).not.toHaveBeenCalled()

    await user.clear(name)
    await user.type(name, 'Blue Lock: Episode Nagi{Enter}')
    await waitFor(() =>
      expect(spies.rename).toHaveBeenCalledWith(blueLock().id, 'Blue Lock: Episode Nagi'),
    )
    // Moving on after Enter commits nothing more.
    await user.tab()
    expect(spies.rename).toHaveBeenCalledTimes(1)
    await waitFor(() =>
      expect(dialog.getByRole('combobox', { name: 'Series' })).toHaveTextContent(
        'Blue Lock: Episode Nagi',
      ),
    )
  })

  it('deletes the series only once it is confirmed', async () => {
    const { files, spies } = contextDesktop()
    const user = userEvent.setup()
    const dialog = await editor()

    await user.click(dialog.getByRole('button', { name: 'Delete series' }))
    let confirm = within(await screen.findByRole('alertdialog', { name: 'Delete series' }))
    expect(
      confirm.getByText('Delete this series and its glossary? Its chapter projects will remain.'),
    ).toBeInTheDocument()
    await user.click(confirm.getByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull())
    expect(spies.delete).not.toHaveBeenCalled()

    await user.click(dialog.getByRole('button', { name: 'Delete series' }))
    confirm = within(await screen.findByRole('alertdialog', { name: 'Delete series' }))
    await user.click(confirm.getByRole('button', { name: 'Delete' }))

    await waitFor(() => expect(spies.delete).toHaveBeenCalledWith(blueLock().id))
    expect(files.series.has(blueLock().id)).toBe(false)
    expect(
      await dialog.findByText('Pick or create a series to keep the glossary across chapters.'),
    ).toBeInTheDocument()
    expect(dialog.queryByRole('textbox', { name: 'Series name' })).toBeNull()
  })

  it('saves the series notes and its characters as they are committed', async () => {
    const { files, saves } = contextDesktop()
    const user = userEvent.setup()
    const dialog = await editor()
    const latest = () => saves.at(-1)!

    await rewrite(user, dialog.getByRole('textbox', { name: 'Series notes' }), 'Football.')
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(latest().notes).toBe('Football.')

    const isagi = within(dialog.getByRole('group', { name: 'Isagi' }))
    await rewrite(user, isagi.getByRole('textbox', { name: 'Voice and pronoun notes' }), 'Ore.')
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(latest().characters).toEqual([
      { id: 'c1', name: 'Isagi', original: '潔', notes: 'Ore.' },
    ])

    await user.click(dialog.getByRole('button', { name: 'Add character' }))
    await waitFor(() => expect(saves).toHaveLength(3))
    const added = within(await dialog.findByRole('group', { name: 'New character' }))
    const name = added.getByRole('textbox', { name: 'Character name' })
    await waitFor(() => expect(name).toHaveFocus())
    await user.type(name, 'Bachira{Enter}')
    await waitFor(() => expect(saves).toHaveLength(4))
    expect(latest().characters!.map((character) => character.name)).toEqual(['Isagi', 'Bachira'])

    await user.click(
      within(await dialog.findByRole('group', { name: 'Isagi' })).getByRole('button', {
        name: 'Delete character',
      }),
    )
    await waitFor(() => expect(saves).toHaveLength(5))
    expect(latest().characters!.map((character) => character.name)).toEqual(['Bachira'])
    expect(files.series.get(blueLock().id)!.notes).toBe('Football.')
  })
})
