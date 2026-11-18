import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { GlossaryDialog } from '@/components/booharu/context/GlossaryDialog'
import { RightSidebar } from '@/components/editor/RightSidebar'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'

import { blueLock, contextDesktop, openChapter } from './context'

type User = ReturnType<typeof userEvent.setup>

/** The glossary editor on Blue Lock, its term 潔 saved again through the term form. */
async function saveIsagi(user: User) {
  render(
    <QueryClientProvider client={queryClient}>
      <GlossaryDialog initial={blueLock().id} onClose={() => undefined} />
    </QueryClientProvider>,
  )
  const dialog = within(await screen.findByRole('dialog', { name: 'Glossary' }))
  const cell = await dialog.findByRole('cell', { name: /^潔/ })
  await user.click(within(cell.closest('tr')!).getByRole('button', { name: 'Edit' }))
  const form = within(await screen.findByRole('dialog', { name: 'Glossary entry' }))
  const target = form.getByRole('textbox', { name: 'Translation' })
  await user.clear(target)
  await user.type(target, 'Yoichi Isagi')
  await user.click(form.getByRole('button', { name: 'Save' }))
  await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Glossary entry' })).toBeNull())
  return dialog
}

/** The right pane, its Context tab shown on the chapter Ch.1. */
async function contextTab(user: User) {
  openChapter()
  useKoharuStore.setState({ panel: 'story' })
  render(
    <QueryClientProvider client={queryClient}>
      <RightSidebar />
    </QueryClientProvider>,
  )
  await user.click(screen.getByRole('tab', { name: 'Context' }))
  const list = await screen.findByRole('list', { name: 'Glossary' })
  return within(
    within(list)
      .getAllByRole('listitem')
      .find((item) => item.textContent?.startsWith('ヒナ →'))!,
  )
}

describe('Booharu term usage and Fix it everywhere', () => {
  it('shows where a term saved in the editor is used, and what Fix it everywhere left alone', async () => {
    const { spies } = contextDesktop()
    spies.usage.mockResolvedValue({
      blocks: ['a', 'b', 'c', 'd'],
      translate: ['a', 'b'],
      hand_edited: 1,
      locked: 1,
    })
    spies.fix.mockResolvedValue({ run: 7, translated: 2, kept_hand_edited: 1, kept_locked: 1 })
    openChapter()
    const user = userEvent.setup()
    const dialog = await saveIsagi(user)

    const usage = within(await dialog.findByRole('group', { name: 'Where 潔 is used' }))
    expect(usage.getByText('潔 → Yoichi Isagi')).toBeInTheDocument()
    expect(await usage.findByText('Used on 4 lines in this project')).toBeInTheDocument()
    expect(usage.getByText('1 hand-edited line will be skipped')).toBeInTheDocument()
    expect(usage.getByText('1 locked line will be skipped')).toBeInTheDocument()
    expect(spies.usage).toHaveBeenCalledWith('潔')

    await user.click(usage.getByRole('button', { name: 'Re-translate those lines' }))
    expect(spies.fix).toHaveBeenCalledExactlyOnceWith('潔')
    const report = within(await usage.findByRole('status'))
    expect(report.getByText('Re-translating 2 lines.')).toBeInTheDocument()
    expect(report.getByText('Skipped 1 hand-edited line.')).toBeInTheDocument()
    expect(report.getByText('Skipped 1 locked line.')).toBeInTheDocument()

    await user.click(usage.getByRole('button', { name: 'Close' }))
    expect(dialog.queryByRole('group', { name: 'Where 潔 is used' })).toBeNull()
  })

  it('shows nothing of the project while none is open', async () => {
    const { saves, spies } = contextDesktop()
    const user = userEvent.setup()
    const dialog = await saveIsagi(user)

    expect(saves).toHaveLength(1)
    expect(dialog.queryByRole('group', { name: 'Where 潔 is used' })).toBeNull()
    expect(spies.usage).not.toHaveBeenCalled()
  })

  it('shows where a suggestion accepted in the Context tab is used, and says why nothing is left', async () => {
    const { spies } = contextDesktop()
    spies.usage.mockResolvedValue({ blocks: ['a', 'b'], translate: [], hand_edited: 1, locked: 1 })
    const user = userEvent.setup()
    const suggestion = await contextTab(user)

    await user.click(suggestion.getByRole('button', { name: 'Accept' }))

    const usage = within(await screen.findByRole('group', { name: 'Where ヒナ is used' }))
    expect(await usage.findByText('Used on 2 lines in this project')).toBeInTheDocument()
    expect(usage.getByText('1 hand-edited line will be skipped')).toBeInTheDocument()
    expect(usage.getByText('1 locked line will be skipped')).toBeInTheDocument()
    expect(
      usage.getByText(
        'Nothing to re-translate: every line using this term was edited by hand or is locked.',
      ),
    ).toBeInTheDocument()
    // With nothing to translate it starts no run.
    expect(usage.getByRole('button', { name: 'Re-translate those lines' })).toBeDisabled()
    expect(spies.fix).not.toHaveBeenCalled()
  })

  it('says why when the lines changed before Fix it everywhere ran, and starts no run', async () => {
    const { spies } = contextDesktop()
    spies.usage.mockResolvedValue({ blocks: ['a'], translate: ['a'], hand_edited: 0, locked: 0 })
    // The line was edited by hand meanwhile, so the desktop found nothing to translate.
    spies.fix.mockResolvedValue({ run: null, translated: 0, kept_hand_edited: 1, kept_locked: 0 })
    const user = userEvent.setup()
    const suggestion = await contextTab(user)

    // Saving an edited suggestion accepts it too.
    await user.click(suggestion.getByRole('button', { name: 'Edit' }))
    const form = within(await screen.findByRole('dialog', { name: 'Glossary entry' }))
    await user.click(form.getByRole('button', { name: 'Save' }))

    const usage = within(await screen.findByRole('group', { name: 'Where ヒナ is used' }))
    expect(await usage.findByText('Used on 1 line in this project')).toBeInTheDocument()
    await user.click(usage.getByRole('button', { name: 'Re-translate those lines' }))
    const report = within(await usage.findByRole('status'))
    expect(
      report.getByText(
        'Nothing to re-translate: every line using this term was edited by hand or is locked.',
      ),
    ).toBeInTheDocument()
    expect(report.getByText('Skipped 1 hand-edited line.')).toBeInTheDocument()
    expect(report.queryByText(/^Re-translating/)).toBeNull()
  })

  it('says no line uses a term the project no longer has', async () => {
    const { spies } = contextDesktop()
    spies.usage.mockResolvedValue({ blocks: ['a'], translate: ['a'], hand_edited: 0, locked: 0 })
    spies.fix.mockResolvedValue({ run: null, translated: 0, kept_hand_edited: 0, kept_locked: 0 })
    const user = userEvent.setup()
    const suggestion = await contextTab(user)

    await user.click(suggestion.getByRole('button', { name: 'Accept' }))
    const usage = within(await screen.findByRole('group', { name: 'Where ヒナ is used' }))
    await user.click(await usage.findByRole('button', { name: 'Re-translate those lines' }))
    expect(
      within(await usage.findByRole('status')).getByText('No line in this project uses this term.'),
    ).toBeInTheDocument()
  })
})
