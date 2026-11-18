import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { ActivityCenter } from '@/components/editor/ActivityCenter'
import { InferenceControl } from '@/components/editor/InferenceControl'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Stage } from '@koharu/ipc/protocol'

import { done, openChapter, pageFailed, queued, tell } from './jobs'
import { settingsDesktop } from './settings'

const timedOut = "The translation model didn't answer within 10 minutes."

/** The Run control, its menu closed. */
function renderRunMenu() {
  vi.spyOn(commands, 'listModels').mockResolvedValue([])
  return render(
    <QueryClientProvider client={queryClient}>
      <InferenceControl onRun={vi.fn()} disabled={false} />
    </QueryClientProvider>,
  )
}

function renderActivity() {
  return render(
    <QueryClientProvider client={queryClient}>
      <ActivityCenter />
    </QueryClientProvider>,
  )
}

/** Run 1 of every stage in Chapter 1, done with problems: page 3 and page 1 failed reading, and
 * page 2 translating, once its other stages had finished. */
function doneWithProblems() {
  const failures = [
    { page: 'p3', stage: 'recognize' as const, reason: 'the model failed' },
    { page: 'p2', stage: 'translate' as const, reason: timedOut },
    { page: 'p1', stage: 'recognize' as const, reason: 'the model failed' },
  ]
  const finished = (stage: Stage) => ({ kind: 'finished' as const, run: 1, page: 'p2', stage })
  openChapter('Chapter 1')
  tell(
    queued(1),
    finished('detect'),
    finished('recognize'),
    finished('inpaint'),
    ...failures.map((failure) => pageFailed(1, failure)),
    done(1, failures),
  )
}

describe('Booharu failed pages in the Activity center', () => {
  it('keeps a run done with problems in view, with each failed page, in its project only', async () => {
    settingsDesktop()
    openChapter('Chapter 2')
    tell(queued(2), {
      kind: 'ended',
      run: 2,
      outcome: { kind: 'failed', page: 'p4', stage: 'inpaint', reason: 'the disk is full' },
    })
    doneWithProblems()
    renderActivity()

    expect(screen.getByText('Finished with problems')).toBeInTheDocument()
    const list = screen.getByRole('list', { name: 'Failed pages' })
    const rows = within(list).getAllByRole('listitem')
    expect(rows.map((row) => row.textContent)).toEqual([
      'Page 3OCRthe model failed',
      `Page 2Translation${timedOut}`,
      'Page 1OCRthe model failed',
    ])
    expect(within(rows[1]!).getByText(timedOut)).toHaveClass('line-clamp-2')
    // Chapter 2's failed run shows only while Chapter 2 is open.
    expect(screen.queryByText('the disk is full')).toBeNull()

    act(() => openChapter('Chapter 2'))
    await waitFor(() => expect(screen.queryByText('Finished with problems')).toBeNull())
    expect(screen.getByText('Processing failed.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Page 4' })).toBeInTheDocument()
  })

  it('shows a failed page as the page rail does', async () => {
    const { show } = { show: vi.fn() }
    settingsDesktop()
    vi.spyOn(commands, 'show').mockImplementation(async (page) => {
      show(page)
      return null
    })
    doneWithProblems()
    useKoharuStore.setState({ selectedLayers: ['layer'] })
    const user = userEvent.setup()
    renderActivity()

    await user.click(screen.getByRole('button', { name: 'Page 2' }))
    expect(useKoharuStore.getState()).toMatchObject({ selectedPages: ['p2'], selectedLayers: [] })
    expect(show).toHaveBeenCalledWith('p2')
  })

  it('retries the failed pages from the stage each failed at, one run per stage', async () => {
    settingsDesktop()
    const run = vi.spyOn(commands, 'runPipeline').mockResolvedValue(5)
    doneWithProblems()
    const user = userEvent.setup()
    renderActivity()

    await user.click(screen.getByRole('button', { name: 'Retry failed pages' }))
    await waitFor(() => expect(screen.queryByText('Finished with problems')).toBeNull())
    expect(run.mock.calls).toEqual([
      [{ pages: ['p1', 'p3'] }, ['recognize', 'inpaint', 'translate']],
      [{ pages: ['p2'] }, ['translate']],
    ])
  })

  it('waits while a run goes, and goes away when dismissed', async () => {
    settingsDesktop()
    doneWithProblems()
    tell(queued(2, ['translate']))
    const user = userEvent.setup()
    renderActivity()

    expect(screen.getByRole('button', { name: 'Retry failed pages' })).toBeDisabled()
    act(() => tell({ kind: 'ended', run: 2, outcome: { kind: 'stopped' } }))
    expect(screen.getByRole('button', { name: 'Retry failed pages' })).toBeEnabled()

    await user.click(screen.getByRole('button', { name: 'Dismiss' }))
    expect(screen.queryByText('Finished with problems')).toBeNull()
    expect(useKoharuStore.getState().jobs['1']).toBeUndefined()
  })

  it('says a running run retries a page while the page waits for its next try', () => {
    settingsDesktop()
    openChapter('Chapter 1')
    tell(
      queued(1),
      { kind: 'started', run: 1, page: 'p1', stage: 'translate' },
      {
        kind: 'retrying',
        run: 1,
        page: 'p1',
        stage: 'translate',
        attempt: 2,
        attempts: 3,
      },
    )
    renderActivity()

    expect(screen.getByRole('status')).toHaveTextContent('Retrying (2/3)')
    act(() => tell({ kind: 'started', run: 1, page: 'p1', stage: 'translate' }))
    expect(screen.queryByText('Retrying (2/3)')).toBeNull()
  })

  it('offers Retry failed pages in the Run menu while the last run here has failed pages', async () => {
    settingsDesktop()
    const run = vi.spyOn(commands, 'runPipeline').mockResolvedValue(5)
    doneWithProblems()
    const user = userEvent.setup()
    renderRunMenu()

    await user.click(screen.getByRole('button', { name: 'Processing settings' }))
    await user.click(await screen.findByRole('button', { name: 'Retry failed pages' }))
    await waitFor(() => expect(run).toHaveBeenCalledTimes(2))
    // The menu closes as the retries go.
    await waitFor(() => expect(screen.queryByRole('group', { name: 'Presets' })).toBeNull())
  })

  it('offers nothing once a later run here ends without problems, or in another project', async () => {
    settingsDesktop()
    doneWithProblems()
    tell(queued(2, ['translate']), done(2))
    const user = userEvent.setup()
    renderRunMenu()

    await user.click(screen.getByRole('button', { name: 'Processing settings' }))
    await screen.findByRole('group', { name: 'Presets' })
    expect(screen.queryByRole('button', { name: 'Retry failed pages' })).toBeNull()

    act(() => useKoharuStore.getState().dismissJob('2'))
    expect(await screen.findByRole('button', { name: 'Retry failed pages' })).toBeInTheDocument()
    act(() => openChapter('Chapter 2'))
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Retry failed pages' })).toBeNull(),
    )
  })
})
