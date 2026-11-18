import { fireEvent, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { BoxDetailsDialog } from '@/components/booharu/details/BoxDetailsDialog'
import { StoryPanel } from '@/components/editor/StoryPanel'
import { openBoxDetails, useBoxDetails } from '@/lib/booharu/boxDetails'
import { letteringKey, setLetteringCategory } from '@/lib/booharu/boxLettering'
import { booharuPreferencesKey } from '@/lib/booharu/preferences'
import { resources, supportedLanguages } from '@/lib/i18n'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { toast } from '@koharu/ui/components/toast'

import { categorySearch, counts, hint, letteredDesktop, mount } from './lettered'

/** Box details' Lettering section, once it shows. */
async function letteringSection() {
  const dialog = await screen.findByRole('dialog', { name: 'Box details' })
  return within(await within(dialog).findByRole('region', { name: 'Lettering' }))
}

beforeEach(() => useBoxDetails.setState({ shown: null, reveal: null }))

describe("Booharu's lettering category in Box details", () => {
  it('says how many locked blocks a change left out', async () => {
    const { setCategory } = letteredDesktop()
    setCategory.mockResolvedValue({ ...counts, locked: 2 })
    const shown = vi.spyOn(toast, 'add')

    await setLetteringCategory(['first', 'locked', 'held-first'], 'automatic')
    expect(setCategory).toHaveBeenCalledWith(['first', 'locked', 'held-first'], null)
    expect(shown).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'info', title: '2 locked boxes kept their category.' }),
    )
  })

  it('shows the picker, why and the bubble shape, its measures in a tooltip', async () => {
    const { setCategory } = letteredDesktop(true)
    const user = userEvent.setup()
    openBoxDetails('page', 'first')
    mount(<BoxDetailsDialog />)

    let section = await letteringSection()
    const picker = section.getByRole('combobox', { name: 'Lettering category' })
    expect(picker).toHaveTextContent('Normal Dialogue')
    expect(section.getByText("From its bubble's shape")).toBeInTheDocument()
    await user.hover(section.getByText('Bubble shape: Round'))
    expect(
      await screen.findByText('Rectangularity 93% · Solidity 95% · Deep dips 0'),
    ).toBeInTheDocument()
    // Pick fonts automatically is on: no hint, once the settings are read.
    await waitFor(() =>
      expect(queryClient.getQueryState(booharuPreferencesKey)?.status).toBe('success'),
    )
    expect(section.queryByText(hint)).toBeNull()

    await user.click(picker)
    await categorySearch()
    await user.click(screen.getByRole('option', { name: 'Thoughts' }))
    await waitFor(() => expect(setCategory).toHaveBeenCalledExactlyOnceWith(['first'], 'thoughts'))

    // Outside any balloon, no bubble and no measures.
    openBoxDetails('page', 'free')
    expect(await screen.findByText('Not in a bubble')).toBeInTheDocument()
    section = await letteringSection()
    expect(section.getByText('No bubble')).toBeInTheDocument()
    expect(section.queryByText(/^Bubble shape/)).toBeNull()
  })

  it('says why the category couldn’t be read, where Story shows no tag', async () => {
    const { lettering: read } = letteredDesktop()
    const refused = "shout's font family can't be blank: leave it out for the default font"
    read.mockRejectedValue(refused)
    mount(
      <>
        <StoryPanel />
        <BoxDetailsDialog />
      </>,
    )

    // Story's rows show no tag, nor keep the line a tag takes while it's read.
    expect(await screen.findByRole('textbox', { name: 'Hel source text' })).toBeInTheDocument()
    await waitFor(() => {
      expect(queryClient.getQueryState(letteringKey('page'))?.status).toBe('error')
      expect(document.querySelector('span.h-4[aria-hidden="true"]')).toBeNull()
    })
    expect(screen.queryAllByRole('combobox', { name: /^Lettering category for/ })).toEqual([])

    openBoxDetails('page', 'first')
    const section = await letteringSection()
    expect(
      await section.findByText(`Couldn't read the lettering category: ${refused}`),
    ).toBeInTheDocument()
    expect(section.queryByRole('combobox', { name: 'Lettering category' })).toBeNull()
  })

  it('leaves the Lettering section out for a text layer', async () => {
    letteredDesktop()
    openBoxDetails('page', 'caption')
    mount(<BoxDetailsDialog />)

    const dialog = await screen.findByRole('dialog', { name: 'Box details' })
    expect(within(dialog).getByRole('region', { name: 'Note' })).toBeInTheDocument()
    expect(within(dialog).queryByRole('region', { name: 'Lettering' })).toBeNull()
  })

  it('says fonts follow categories only with Pick fonts automatically on, linking to it', async () => {
    letteredDesktop(false)
    const user = userEvent.setup()
    openBoxDetails('page', 'first')
    mount(<BoxDetailsDialog />)

    const note = await (await letteringSection()).findByText(hint)
    await user.click(within(note).getByRole('button', { name: 'Pick fonts automatically' }))
    expect(useKoharuStore.getState().settingsOpen).toBe(true)
  })

  it('opens on the Lettering section from Change category', async () => {
    const { lettering: read } = letteredDesktop()
    const user = userEvent.setup()
    mount(
      <>
        <StoryPanel />
        <BoxDetailsDialog />
      </>,
    )

    await user.click(await screen.findByRole('button', { name: 'Actions for Hel' }))
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Change category…' }))
    const section = await letteringSection()
    expect(useBoxDetails.getState().shown).toEqual({
      page: 'page',
      layer: 'first',
      section: 'lettering',
    })
    expect(read).toHaveBeenCalledWith('page')
    await waitFor(() =>
      expect(section.getByRole('combobox', { name: 'Lettering category' })).toHaveFocus(),
    )
  })

  it('names the undo step in every language', () => {
    for (const language of supportedLanguages) {
      const { history } = resources[language].translation as {
        history: { steps: Record<string, string> }
      }
      expect(history.steps.booharu_set_lettering_category, language).toMatch(/\S/)
    }
  })
})
