import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { TFunction } from 'i18next'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { RightSidebar } from '@/components/editor/RightSidebar'
import { qcGroups, qcLabel, useBooharuQcEvents } from '@/lib/booharu/qc'
import { projectKey, queryClient, type ProjectView } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/ipc/protocol'

import { finding, forgetFindings, listenToEvents, qcDesktop, settle, tell } from './qc'

const overflow = finding('p1', 'a')
const flag = finding('p1', 'b', { kind: 'flag', message: 'Check the name' })
const small = finding('p2', 'c', { kind: 'too_small', size: 9.5, limit: 12 }, { hidden: true })
const missing = finding('p2', 'd', { kind: 'missing_translation' }, { text: '' })

/** The project's pages in their order now: p2 before p1. */
const order = ['p2', 'p1', 'p3']

/** The project open with p2 then p1, the canvas showing `shown`. */
function openDemo(shown: string) {
  const size = { width: 800, height: 1200 }
  const project: ProjectView = {
    name: 'Demo',
    page: shown,
    pages: order.map((id) => ({ id, size, background: null, texts: 0, translated: 0 })),
    history: { steps: [], current: 0 },
    revisions: {},
  }
  queryClient.setQueryData(projectKey, project)
}

/** The app's listener for the findings, as the providers mount it. */
function Events() {
  useBooharuQcEvents()
  return null
}

/** The right pane's tabs' names. */
const tabs = () => screen.getAllByRole('tab').map((tab) => tab.textContent?.trim())

beforeEach(() => {
  listenToEvents()
  forgetFindings()
})

describe('Booharu QC tab', () => {
  it('groups the rows by page in the project’s order, under each page’s number now', () => {
    const pages = { p1: [overflow, flag], p2: [small, missing], gone: [finding('gone', 'x')] }

    expect(qcGroups(pages, order, 'all', false)).toEqual([
      { page: 'p2', number: 1, findings: [missing] },
      { page: 'p1', number: 2, findings: [overflow, flag] },
    ])
    expect(qcGroups(pages, order, 'all', true)[0]).toEqual({
      page: 'p2',
      number: 1,
      findings: [small, missing],
    })
    expect(qcGroups(pages, order, 'agent', false)).toEqual([
      { page: 'p1', number: 2, findings: [flag] },
    ])
    expect(qcGroups(pages, order, 'auto', false).map((group) => group.findings)).toEqual([
      [missing],
      [overflow],
    ])
    // A page whose findings are all marked OK shows only with Show hidden.
    expect(qcGroups({ p2: [small] }, order, 'all', false)).toEqual([])
  })

  it('names what each finding found, the agent’s flag in its own words', () => {
    const t = ((key: string, options?: object) =>
      options ? `${key} ${JSON.stringify(options)}` : key) as unknown as TFunction
    const label = (problem: Parameters<typeof finding>[2]) => qcLabel(t, finding('p', 'l', problem))

    expect(label({ kind: 'leftover_source', unchanged: false })).toBe(
      'booharu.qc.checks.leftover_source',
    )
    expect(label({ kind: 'leftover_source', unchanged: true })).toBe('booharu.qc.checks.unchanged')
    expect(label({ kind: 'odd_font', expected: null, font: 'Comic' })).toBe(
      'booharu.qc.checks.odd_font {"expected":"booharu.qc.defaultFont"}',
    )
    expect(label({ kind: 'small_word', word: 'ドン', size: 7.5, limit: 12 })).toBe(
      'booharu.qc.checks.small_word {"size":7.5,"word":"ドン"}',
    )
    expect(label({ kind: 'glossary', source: '潔', target: 'Isagi' })).toBe(
      'booharu.qc.checks.glossary {"source":"潔","target":"Isagi"}',
    )
    expect(qcLabel(t, flag)).toBe('Check the name')
  })

  it('shows last while a finding is open, and shows a row’s box without leaving the tab', async () => {
    qcDesktop('Demo', { Demo: [overflow, flag, small, missing] })
    const show = vi.mocked(commands.show)
    openDemo('p1')
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <Events />
        <RightSidebar />
      </QueryClientProvider>,
    )
    expect(tabs()).toEqual(['Layers', 'Story', 'Context', 'Agent'])
    await settle()
    expect(tabs()).toEqual(['Layers', 'Story', 'Context', 'Agent', 'QC 3'])

    await user.click(screen.getByRole('tab', { name: 'QC 3' }))
    const qc = within(screen.getByRole('region', { name: 'QC' }))
    expect(qc.getAllByRole('heading', { level: 3 }).map((heading) => heading.textContent)).toEqual([
      'Page 1',
      'Page 2',
    ])
    expect(
      within(qc.getByRole('region', { name: 'Page 1' })).getByText('No translation'),
    ).toBeInTheDocument()
    // The line and the agent's words keep their own direction in a right-to-left interface.
    expect(qc.getByText('Check the name').closest('bdi')).not.toBeNull()
    expect(qc.getByText('a text').closest('bdi')).not.toBeNull()
    expect(qc.getByText('Text overflows the box').closest('bdi')).toBeNull()

    // A row on the page shown selects its box at once.
    await user.click(qc.getByRole('button', { name: 'Text overflows the box a text' }))
    expect(useKoharuStore.getState()).toMatchObject({ panel: 'qc', selectedLayers: ['a'] })
    expect(show).not.toHaveBeenCalled()

    // A row on another page shows that page first.
    await user.click(qc.getByRole('button', { name: 'Missing translation No translation' }))
    expect(show).toHaveBeenCalledWith('p2')
    await waitFor(() =>
      expect(useKoharuStore.getState()).toMatchObject({
        panel: 'qc',
        selectedPages: ['p2'],
        selectedLayers: ['d'],
      }),
    )
  })

  it('ends on the box pressed last while another row’s page is still being shown', async () => {
    const empty = finding('p2', 'e', { kind: 'empty_source' })
    qcDesktop('Demo', { Demo: [overflow, missing, empty] })
    const show = vi.mocked(commands.show)
    /** Holds the next page show until the returned function lets it end. */
    const held = () => {
      let end: () => void = () => undefined
      show.mockImplementationOnce(() => new Promise((resolve) => (end = () => resolve(null))))
      return () => act(() => end())
    }
    openDemo('p1')
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <Events />
        <RightSidebar />
      </QueryClientProvider>,
    )
    await settle()
    await user.click(screen.getByRole('tab', { name: 'QC 3' }))
    const qc = within(screen.getByRole('region', { name: 'QC' }))

    // Two rows on p2, the second pressed while the page is on its way.
    let end = held()
    await user.click(qc.getByRole('button', { name: 'Missing translation No translation' }))
    await user.click(qc.getByRole('button', { name: 'Empty OCR result e text' }))
    expect(show).toHaveBeenCalledTimes(1)
    end()
    await waitFor(() => expect(useKoharuStore.getState().selectedLayers).toEqual(['e']))

    // The row on the page the canvas shows, pressed while p2 is on its way, shows its page again.
    end = held()
    await user.click(qc.getByRole('button', { name: 'Missing translation No translation' }))
    await user.click(qc.getByRole('button', { name: 'Text overflows the box a text' }))
    expect(useKoharuStore.getState()).toMatchObject({ selectedPages: ['p1'], selectedLayers: [] })
    end()
    await waitFor(() =>
      expect(useKoharuStore.getState()).toMatchObject({
        selectedPages: ['p1'],
        selectedLayers: ['a'],
      }),
    )
    expect(show.mock.calls).toEqual([['p2'], ['p2'], ['p2'], ['p1']])
  })

  it('marks a finding OK, takes it back, dismisses a flag and filters the rows', async () => {
    qcDesktop('Demo', { Demo: [overflow, flag, small, missing] })
    const mark = vi.mocked(commands.booharuMarkQcOk)
    const dismiss = vi.mocked(commands.booharuDismissFlag)
    openDemo('p1')
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <Events />
        <RightSidebar />
      </QueryClientProvider>,
    )
    await settle()
    await user.click(screen.getByRole('tab', { name: 'QC 3' }))
    const qc = within(screen.getByRole('region', { name: 'QC' }))

    await user.click(qc.getByRole('button', { name: 'Mark Text overflows the box OK' }))
    expect(mark).toHaveBeenCalledWith('a', 'overflow', true)
    // The desktop checks the page again at once.
    tell('booharuQc', { page: 'p1', findings: [{ ...overflow, hidden: true }, flag] })
    expect(screen.getByRole('tab', { name: 'QC 2' })).toBeInTheDocument()
    expect(qc.queryByRole('button', { name: 'Text overflows the box a text' })).toBeNull()

    await user.click(qc.getByRole('checkbox', { name: 'Show hidden' }))
    await user.click(qc.getByRole('button', { name: 'Undo OK for Text overflows the box' }))
    expect(mark).toHaveBeenLastCalledWith('a', 'overflow', false)
    expect(qc.getByRole('button', { name: 'Undo OK for Small text: 9.5 px' })).toBeInTheDocument()

    await user.click(qc.getByRole('button', { name: 'Dismiss review flag: Check the name' }))
    expect(dismiss).toHaveBeenCalledWith('b', 'Check the name')

    await user.click(qc.getByRole('button', { name: 'Agent' }))
    expect(qc.getAllByRole('heading', { level: 3 }).map((heading) => heading.textContent)).toEqual([
      'Page 2',
    ])
    expect(qc.queryByRole('button', { name: /Text overflows the box/ })).toBeNull()
    await user.click(qc.getByRole('button', { name: 'Auto checks' }))
    expect(qc.queryByRole('button', { name: /Check the name/ })).toBeNull()
    expect(
      qc.getByRole('button', { name: 'Undo OK for Text overflows the box' }),
    ).toBeInTheDocument()

    // Once nothing is open, the tab hides and the pane shows the tab before it.
    tell('booharuQc', { page: 'p1', findings: [{ ...overflow, hidden: true }] })
    tell('booharuQc', { page: 'p2', findings: [small] })
    expect(tabs()).toEqual(['Layers', 'Story', 'Context', 'Agent'])
    expect(useKoharuStore.getState().panel).toBe('layers')
  })
})
