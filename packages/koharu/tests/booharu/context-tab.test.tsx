import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { RightSidebar } from '@/components/editor/RightSidebar'
import { refreshContext } from '@/lib/booharu/context'
import { resources, supportedLanguages } from '@/lib/i18n'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'

import { contextDesktop, haikyu, openChapter, term } from './context'

/** The right pane, its Context tab shown. */
async function contextTab(user: ReturnType<typeof userEvent.setup>) {
  openChapter()
  useKoharuStore.setState({ panel: 'story' })
  render(
    <QueryClientProvider client={queryClient}>
      <RightSidebar />
    </QueryClientProvider>,
  )
  await user.click(screen.getByRole('tab', { name: 'Context' }))
}

/** The glossary list's row of the term `source`. */
async function row(source: string) {
  const list = await screen.findByRole('list', { name: 'Glossary' })
  const item = within(list)
    .getAllByRole('listitem')
    .find((candidate) => candidate.textContent?.startsWith(`${source} →`))
  if (!item) throw new Error(`no row for ${source}`)
  return within(item)
}

describe('Booharu Context tab', () => {
  it('sits between Story and Agent, and lists the series’ terms, then the suggestions', async () => {
    contextDesktop()
    const user = userEvent.setup()
    await contextTab(user)

    expect(screen.getAllByRole('tab').map((tab) => tab.textContent?.trim())).toEqual([
      'Layers',
      'Story',
      'Context',
      'Agent',
    ])
    const list = await screen.findByRole('list', { name: 'Glossary' })
    expect(
      within(list)
        .getAllByRole('listitem')
        .map((item) => item.querySelector('p')?.textContent),
    ).toEqual(['潔 → Isagi', 'エゴ → ego', 'ヒナ → Hina'])
    expect(screen.getByRole('combobox', { name: 'Series' })).toHaveTextContent('Blue Lock')

    const locked = await row('潔')
    expect(locked.getByRole('img', { name: 'Locked' })).toBeInTheDocument()
    expect(locked.getByText('Name')).toBeInTheDocument()
    expect(locked.queryByText('Suggested')).toBeNull()
    expect(locked.queryByRole('button')).toBeNull()
    const suggested = await row('ヒナ')
    expect(suggested.getByText('Suggested')).toBeInTheDocument()
    expect(suggested.queryByRole('img', { name: 'Locked' })).toBeNull()

    await user.type(screen.getByRole('searchbox', { name: 'Search glossary' }), 'EGO')
    expect(
      within(list)
        .getAllByRole('listitem')
        .map((item) => item.querySelector('p')?.textContent),
    ).toEqual(['エゴ → ego'])
    await user.clear(screen.getByRole('searchbox', { name: 'Search glossary' }))
    await user.type(screen.getByRole('searchbox', { name: 'Search glossary' }), 'Nagi')
    expect(screen.getByText('No terms match your search.')).toBeInTheDocument()
  })

  it('accepts a suggestion into the series unlocked, through the desktop', async () => {
    const { files, spies } = contextDesktop()
    const user = userEvent.setup()
    await contextTab(user)

    await user.click((await row('ヒナ')).getByRole('button', { name: 'Accept' }))

    await waitFor(() =>
      expect(spies.accept).toHaveBeenCalledWith(
        { id: 's1', source: 'ヒナ', target: 'Hina', note: '', kind: 'name', locked: false },
        false,
      ),
    )
    expect(files.chapter.suggestions).toEqual([])
    // It shows as the series' own once the chapter is read again.
    const accepted = await row('ヒナ')
    await waitFor(() => expect(accepted.queryByText('Suggested')).toBeNull())
  })

  it('edits a suggestion in the term form, saving it accepts it as edited', async () => {
    const { spies } = contextDesktop()
    const user = userEvent.setup()
    await contextTab(user)

    await user.click((await row('ヒナ')).getByRole('button', { name: 'Edit' }))
    const form = within(await screen.findByRole('dialog', { name: 'Glossary entry' }))
    expect(form.getByRole('textbox', { name: 'Source' })).toHaveValue('ヒナ')
    const target = form.getByRole('textbox', { name: 'Translation' })
    await user.clear(target)
    await user.type(target, 'Hinata')
    await user.click(form.getByRole('checkbox', { name: 'Lock this translation' }))
    await user.click(form.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(spies.accept).toHaveBeenCalledWith(
        expect.objectContaining({ id: 's1', source: 'ヒナ', target: 'Hinata', locked: true }),
        false,
      ),
    )
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  })

  it('accepts a suggestion over a term of its source only through the form, which says so', async () => {
    // Suggested before the user locked 潔 → Isagi in the series.
    const { spies } = contextDesktop({
      suggestions: [term('s2', '潔', 'Yoichi', { kind: 'name' })],
    })
    const user = userEvent.setup()
    await contextTab(user)

    const list = await screen.findByRole('list', { name: 'Glossary' })
    const suggested = within(list)
      .getAllByRole('listitem')
      .find((item) => item.textContent?.startsWith('潔 → Yoichi'))
    await user.click(within(suggested!).getByRole('button', { name: 'Accept' }))
    const form = within(await screen.findByRole('dialog', { name: 'Glossary entry' }))
    expect(form.getByText('Saving replaces 潔 → Isagi.')).toBeInTheDocument()
    expect(form.getByRole('textbox', { name: 'Translation' })).toHaveValue('Yoichi')
    expect(spies.accept).not.toHaveBeenCalled()

    await user.click(form.getByRole('button', { name: 'Save' }))
    // The user chose to replace the locked term, as the form said saving does.
    await waitFor(() =>
      expect(spies.accept).toHaveBeenCalledWith(
        expect.objectContaining({ id: 's2', source: '潔', target: 'Yoichi', locked: false }),
        true,
      ),
    )
  })

  it('dismisses a suggestion through the desktop', async () => {
    const { spies } = contextDesktop()
    const user = userEvent.setup()
    await contextTab(user)

    await user.click((await row('ヒナ')).getByRole('button', { name: 'Dismiss' }))

    await waitFor(() => expect(spies.dismiss).toHaveBeenCalledWith('s1'))
    const list = await screen.findByRole('list', { name: 'Glossary' })
    await waitFor(() => expect(within(list).getAllByRole('listitem')).toHaveLength(2))
    expect(spies.accept).not.toHaveBeenCalled()
  })

  it('opens the glossary editor on the linked series, and closes it', async () => {
    contextDesktop()
    const user = userEvent.setup()
    await contextTab(user)

    await user.click(await screen.findByRole('button', { name: 'Open full glossary' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Glossary' }))
    expect(await dialog.findByRole('textbox', { name: 'Series name' })).toHaveValue('Blue Lock')
    await user.click(dialog.getByRole('button', { name: 'Close' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  })

  it('asks for a series, links the one picked, and links a new one it makes', async () => {
    const { files, spies } = contextDesktop({ linked: null })
    const user = userEvent.setup()
    await contextTab(user)

    expect(
      await screen.findByText('Pick or create a series to keep the glossary across chapters.'),
    ).toBeInTheDocument()
    // With no series linked, a suggestion can only be dismissed.
    const suggested = await row('ヒナ')
    expect(suggested.getByRole('button', { name: 'Accept' })).toBeDisabled()
    expect(suggested.getByRole('button', { name: 'Edit' })).toBeDisabled()

    await user.click(screen.getByRole('combobox', { name: 'Series' }))
    await user.click(await screen.findByRole('option', { name: 'Haikyu' }))
    await waitFor(() => expect(spies.link).toHaveBeenCalledWith(haikyu().id))
    await waitFor(() =>
      expect(screen.getByRole('combobox', { name: 'Series' })).toHaveTextContent('Haikyu'),
    )
    expect(
      screen.queryByText('Pick or create a series to keep the glossary across chapters.'),
    ).toBeNull()

    await user.click(screen.getByRole('button', { name: 'New series…' }))
    await user.type(screen.getByRole('textbox', { name: 'Series name' }), ' Kagurabachi {Enter}')
    await waitFor(() => expect(spies.create).toHaveBeenCalledWith('Kagurabachi'))
    await waitFor(() => expect(spies.link).toHaveBeenLastCalledWith('new-series-000000000000'))
    expect(files.chapter.series).toBe('new-series-000000000000')
    await waitFor(() =>
      expect(screen.getByRole('combobox', { name: 'Series' })).toHaveTextContent('Kagurabachi'),
    )
    expect(screen.queryByRole('textbox', { name: 'Series name' })).toBeNull()
  })

  it('says when the linked series is gone, and shows a side file’s error', async () => {
    const { files, spies } = contextDesktop()
    files.series.delete('blue-lock-0123456789ab')
    const user = userEvent.setup()
    await contextTab(user)

    expect(
      await screen.findByText(
        'The series this chapter followed is gone. Pick another or create one.',
      ),
    ).toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: 'Series' })).toHaveTextContent('Pick series')

    // The side file is broken while the tab shows, then a change reads the chapter again.
    spies.chapter.mockRejectedValue('Ch.1.booharu.json is not a chapter file: expected value')
    await act(() => refreshContext())
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Could not load chapter context.')
    expect(alert).toHaveTextContent('Ch.1.booharu.json is not a chapter file: expected value')
  })

  it('has its strings in every language', () => {
    const keys = (value: object): string[] =>
      Object.entries(value).flatMap(([key, item]) =>
        typeof item === 'string' ? [key] : keys(item).map((inner) => `${key}.${inner}`),
      )
    const context = (language: string) =>
      (resources[language].translation as { booharu: { context: object } }).booharu.context
    const english = keys(context('en-US')).sort()
    expect(english).toContain('openGlossary')
    // A language adds the plural forms its counts need, such as Russian's few and many.
    const pluralForm = /_(zero|one|two|few|many|other)$/
    const isForm = (key: string) =>
      pluralForm.test(key) && english.includes(key.replace(pluralForm, '_other'))
    for (const language of supportedLanguages) {
      const own = keys(context(language))
      expect(
        english.filter((key) => !own.includes(key)),
        language,
      ).toEqual([])
      expect(
        own.filter((key) => !english.includes(key) && !isForm(key)),
        language,
      ).toEqual([])
    }
  })
})
