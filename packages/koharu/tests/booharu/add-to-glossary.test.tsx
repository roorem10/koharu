import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { BoxDetailsDialog } from '@/components/booharu/details/BoxDetailsDialog'
import { StoryPanel } from '@/components/editor/StoryPanel'
import { openBoxDetails, useBoxDetails } from '@/lib/booharu/boxDetails'
import { queryClient } from '@/lib/queries'
import { commands, type Page } from '@koharu/ipc/protocol'

import { block, boxesPage, openProject } from './boxes'
import { blueLock, contextDesktop, haikyu } from './context'

type User = ReturnType<typeof userEvent.setup>

function mount(ui: ReactNode) {
  render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>)
}

/** The boxes page, its free block reading 潔の武器, translated "Isagi's weapon". */
function storyPage(): Page {
  const page = boxesPage()
  page.layers = page.layers.map((layer) =>
    layer.id === 'free'
      ? block('free', '潔の武器', { translation: { content: [{ insert: "Isagi's weapon" }] } })
      : layer,
  )
  return page
}

/** The context commands, with the chapter following `linked`, and the boxes page open in Story. */
function setUp(linked: string | null = blueLock().id) {
  const desktop = contextDesktop({ linked })
  vi.spyOn(commands, 'booharuGetLettering').mockResolvedValue([])
  openProject(storyPage())
  return desktop
}

/** Right-clicks `field` with `from` to `to` of its text selected, then picks Add to glossary. */
async function addToGlossary(user: User, field: HTMLElement, from = 0, to = 0) {
  if (field instanceof HTMLTextAreaElement) field.setSelectionRange(from, to)
  fireEvent.contextMenu(field, { button: 2, clientX: 20, clientY: 30 })
  await user.click(await screen.findByRole('menuitem', { name: 'Add to glossary' }))
}

/** The term form, once it shows. */
async function termForm() {
  return within(await screen.findByRole('dialog', { name: 'Glossary entry' }))
}

beforeEach(() => useBoxDetails.setState({ shown: null, reveal: null }))

describe('Booharu Add to glossary', () => {
  it('fills the term form from the source selected, and saves in place of the same source', async () => {
    const { saves, spies } = setUp()
    spies.usage.mockResolvedValue({
      blocks: ['free', 'alone'],
      translate: ['free'],
      hand_edited: 1,
      locked: 0,
    })
    const user = userEvent.setup()
    mount(<StoryPanel />)

    const source = await screen.findByRole('textbox', { name: "Isagi's weapon source text" })
    await addToGlossary(user, source, 0, 1)

    const form = await termForm()
    expect(form.getByRole('textbox', { name: 'Source' })).toHaveValue('潔')
    expect(form.getByRole('textbox', { name: 'Translation' })).toHaveValue("Isagi's weapon")
    // The series already has a term for 潔, which saving replaces.
    expect(form.getByText('Saving replaces 潔 → Isagi.')).toBeInTheDocument()
    const target = form.getByRole('textbox', { name: 'Translation' })
    await user.clear(target)
    await user.type(target, 'Yoichi Isagi')
    await user.click(form.getByRole('checkbox', { name: 'Lock this translation' }))
    await user.click(form.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]!.id).toBe(blueLock().id)
    expect(saves[0]!.glossary).toEqual([
      { id: 't1', source: '潔', target: 'Yoichi Isagi', note: '', kind: 'term', locked: true },
      blueLock().glossary[1],
    ])

    // Then it shows where the term is used, until Done.
    const saved = within(await screen.findByRole('dialog', { name: 'Term saved' }))
    const usage = within(saved.getByRole('group', { name: 'Where 潔 is used' }))
    expect(await usage.findByText('Used on 2 lines in this project')).toBeInTheDocument()
    expect(usage.getByText('1 hand-edited line will be skipped')).toBeInTheDocument()
    expect(usage.getByRole('button', { name: 'Re-translate those lines' })).toBeEnabled()
    expect(spies.usage).toHaveBeenCalledWith('潔')
    await user.click(saved.getByRole('button', { name: 'Done' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  })

  it('fills the translation selected, and the line’s whole source, as a new term', async () => {
    const { saves } = setUp()
    const user = userEvent.setup()
    mount(<StoryPanel />)

    const translation = await screen.findByRole('textbox', { name: "Isagi's weapon translation" })
    await addToGlossary(user, translation, 8, 14)

    const form = await termForm()
    expect(form.getByRole('textbox', { name: 'Source' })).toHaveValue('潔の武器')
    expect(form.getByRole('textbox', { name: 'Translation' })).toHaveValue('weapon')
    expect(form.queryByText(/^Saving replaces/)).toBeNull()
    expect(form.getByRole('checkbox', { name: 'Lock this translation' })).not.toBeChecked()
    await user.click(form.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]!.glossary).toEqual([
      ...blueLock().glossary,
      expect.objectContaining({
        source: '潔の武器',
        target: 'weapon',
        kind: 'term',
        locked: false,
      }),
    ])
    expect(await screen.findByRole('dialog', { name: 'Term saved' })).toBeInTheDocument()
  })

  it('takes the whole line when nothing is selected, and Cancel saves nothing', async () => {
    const { saves } = setUp()
    const user = userEvent.setup()
    mount(<StoryPanel />)

    await addToGlossary(
      user,
      await screen.findByRole('textbox', { name: "Isagi's weapon source text" }),
    )
    const form = await termForm()
    expect(form.getByRole('textbox', { name: 'Source' })).toHaveValue('潔の武器')
    expect(form.getByRole('textbox', { name: 'Translation' })).toHaveValue("Isagi's weapon")
    await user.click(form.getByRole('button', { name: 'Cancel' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
    expect(saves).toEqual([])
  })

  it('asks for a series first when the chapter follows none, then saves into it', async () => {
    const { files, saves, spies } = setUp(null)
    const user = userEvent.setup()
    mount(<StoryPanel />)

    await addToGlossary(
      user,
      await screen.findByRole('textbox', { name: "Isagi's weapon translation" }),
    )
    const ask = within(await screen.findByRole('dialog', { name: 'Add to glossary' }))
    expect(
      ask.getByText('Pick or create a series to keep the glossary across chapters.'),
    ).toBeInTheDocument()
    await user.click(await ask.findByRole('combobox', { name: 'Series' }))
    await user.click(await screen.findByRole('option', { name: 'Haikyu' }))

    await waitFor(() => expect(spies.link).toHaveBeenCalledWith(haikyu().id))
    expect(files.chapter.series).toBe(haikyu().id)
    const form = await termForm()
    expect(form.getByRole('textbox', { name: 'Source' })).toHaveValue('潔の武器')
    await user.click(form.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]!.id).toBe(haikyu().id)
    expect(saves[0]!.glossary).toEqual([
      expect.objectContaining({ source: '潔の武器', target: "Isagi's weapon" }),
    ])
  })

  it('asks for another series when the linked one’s file doesn’t read', async () => {
    const { files, spies } = setUp()
    files.unreadable.add(blueLock().id)
    const user = userEvent.setup()
    mount(<StoryPanel />)

    await addToGlossary(
      user,
      await screen.findByRole('textbox', { name: "Isagi's weapon source text" }),
    )
    const ask = within(await screen.findByRole('dialog', { name: 'Add to glossary' }))
    expect(
      ask.getByText(
        "The series this chapter follows can't be read. Fix its file or pick another series.",
      ),
    ).toBeInTheDocument()
    expect(ask.getByRole('alert')).toHaveTextContent('blue-lock-0123456789ab.json')
    await user.click(await ask.findByRole('combobox', { name: 'Series' }))
    await user.click(await screen.findByRole('option', { name: 'Haikyu' }))

    await waitFor(() => expect(spies.link).toHaveBeenCalledWith(haikyu().id))
    const form = await termForm()
    expect(form.getByRole('textbox', { name: 'Source' })).toHaveValue('潔の武器')
  })

  it('opens from the texts Box details shows too, with the text selected there', async () => {
    const { saves } = setUp()
    const user = userEvent.setup()
    mount(<BoxDetailsDialog />)

    act(() => openBoxDetails('page', 'free'))
    const dialog = within(await screen.findByRole('dialog', { name: 'Box details' }))
    const translation = dialog.getByText("Isagi's weapon")
    const range = document.createRange()
    range.setStart(translation.firstChild!, 0)
    range.setEnd(translation.firstChild!, 5)
    window.getSelection()!.removeAllRanges()
    window.getSelection()!.addRange(range)
    await addToGlossary(user, translation)

    const form = await termForm()
    expect(form.getByRole('textbox', { name: 'Source' })).toHaveValue('潔の武器')
    expect(form.getByRole('textbox', { name: 'Translation' })).toHaveValue('Isagi')
    await user.click(form.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]!.glossary!.at(-1)).toEqual(
      expect.objectContaining({ source: '潔の武器', target: 'Isagi' }),
    )
  })
})
