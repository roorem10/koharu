import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { historyChangedEvent } from '@/components/context/events'
import { RightSidebar } from '@/components/editor/RightSidebar'
import { QcPreferences } from '@/components/preferences/QcPreferences'
import { BoxQcSection } from '@/components/qc/BoxQcSection'
import { useAutomaticQc, qcKey, qcSettingsKey } from '@/components/qc/data'
import { QcPanel } from '@/components/qc/QcPanel'
import { useBoxIssues } from '@/components/qc/useBoxIssues'
import { pageKey, pagesKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type Page,
  type ProjectInfo,
  type QcFinding,
  type QcReport,
} from '@koharu/bridge/protocol'

vi.mock('@tauri-apps/api/core', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@tauri-apps/api/core')>()),
  Channel: class<T> {
    onmessage = (_value: T) => {}
  },
}))
vi.mock('@koharu/bridge/canvas', () => ({ showCanvasPage: vi.fn(() => false) }))
vi.mock('@/components/editor/Inspector', () => ({ Inspector: () => <div>Inspector content</div> }))
vi.mock('@/components/editor/AgentPanel', () => ({ AgentPanel: () => <div>Agent content</div> }))

const project: ProjectInfo = {
  name: 'QC test',
  revision: 1,
  active_page: 'page-1',
  can_undo: false,
  can_redo: false,
}
const page: Page = {
  id: 'page-1',
  label: '01.png',
  size: { width: 1000, height: 1400 },
  layers: [],
  regions: [],
}
const finding: QcFinding = {
  page: 'page-2',
  page_label: '02.png',
  layer: 'box-2',
  check: 'leftover_source',
  detail: '',
  translation: 'Hello 世界',
  flag: null,
}
const initial: QcReport = { revision: 1, findings: [finding], hidden: [], pages_checked: 2 }
const key = qcKey(project.name)
function setup(report = initial) {
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pageKey, page)
  queryClient.setQueryData(pagesKey, [{ id: 'page-1' }, { id: 'page-2' }])
  queryClient.setQueryData(key, report)
  vi.mocked(commands.getQcResults).mockResolvedValue(report)
}
function mount(element: React.ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{element}</QueryClientProvider>)
}

function Counts() {
  const counts = useBoxIssues('page-2')
  return <output>{counts.get('box-2') ?? 0}</output>
}

function Auto() {
  useAutomaticQc()
  return null
}

describe('automatic QC', () => {
  it('opens findings through the shared page and layer selection', async () => {
    setup()
    const next = { ...page, id: 'page-2', label: '02.png' }
    vi.spyOn(commands, 'selectPage').mockResolvedValue({
      project: { ...project, active_page: 'page-2' },
      page: next,
    })
    mount(<QcPanel />)
    fireEvent.click(screen.getByRole('button', { name: /Japanese remains.*Hello 世界/ }))
    await waitFor(() => expect(useKoharuStore.getState().selectedLayers).toEqual(['box-2']))
    expect(useKoharuStore.getState().selectedPages).toEqual(['page-2'])
    expect(queryClient.getQueryData(pageKey)).toEqual(next)
  })

  it('shares one results query across consumers and counts only open findings on the requested page', async () => {
    setup({
      ...initial,
      findings: [
        finding,
        { ...finding, check: 'overflow' },
        { ...finding, page: 'page-1', layer: 'other' },
      ],
      hidden: [{ ...finding, check: 'too_small' }],
    })
    queryClient.removeQueries({ queryKey: key })
    mount(
      <>
        <Counts />
        <Counts />
      </>,
    )
    await waitFor(() => expect(screen.getAllByText('2')).toHaveLength(2))
    expect(commands.getQcResults).toHaveBeenCalledOnce()
    act(() => queryClient.setQueryData(key, { ...initial, findings: [], hidden: [finding] }))
    await waitFor(() => expect(screen.getAllByText('0')).toHaveLength(2))
  })

  it('marks OK and restores hidden findings from the box details', async () => {
    setup()
    mount(<BoxQcSection layer='box-2' />)
    vi.mocked(commands.getQcResults).mockResolvedValue({
      ...initial,
      findings: [],
      hidden: [finding],
    })
    fireEvent.click(screen.getByRole('button', { name: /Mark Japanese remains.*OK/ }))
    await screen.findByRole('button', { name: /Undo OK for Japanese remains/ })
    expect(commands.markQcOk).toHaveBeenCalledWith('box-2', 'leftover_source', true)
    vi.mocked(commands.getQcResults).mockResolvedValue(initial)
    fireEvent.click(screen.getByRole('button', { name: /Undo OK for Japanese remains/ }))
    await screen.findByRole('button', { name: /Mark Japanese remains.*OK/ })
    expect(commands.markQcOk).toHaveBeenLastCalledWith('box-2', 'leftover_source', false)
  })

  it('acknowledges a specific agent note without deleting it and filters agent findings', async () => {
    const flag = { kind: 'review', message: 'Check the joke', page: 2 } as const
    const note = { ...finding, check: 'flag', detail: flag.message, flag } as QcFinding
    setup({ ...initial, findings: [finding, note] })
    mount(<QcPanel />)
    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(screen.queryByText('Japanese remains in the translation')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Mark Check the joke OK' }))
    await waitFor(() =>
      expect(commands.markQcFinding).toHaveBeenCalledWith('box-2', 'flag', flag, true),
    )
  })

  it('hides an empty QC tab, shows its count, leaves other tabs alone and returns to Properties when cleared', async () => {
    setup({ ...initial, findings: [] })
    mount(<RightSidebar />)
    const tabs = within(screen.getByRole('group', { name: 'Editor panels' }))
    expect(tabs.queryByRole('button', { name: /^QC/ })).not.toBeInTheDocument()
    fireEvent.click(tabs.getByRole('button', { name: /^Agent/ }))
    act(() => queryClient.setQueryData(key, initial))
    await waitFor(() => expect(tabs.getByRole('button', { name: /^QC 1$/ })).toBeInTheDocument())
    expect(tabs.getByRole('button', { name: /^Agent/ })).toHaveAttribute('aria-pressed', 'true')
    fireEvent.click(tabs.getByRole('button', { name: /^QC 1$/ }))
    expect(screen.getByText('Hello 世界')).toBeVisible()
    act(() => queryClient.setQueryData(key, { ...initial, findings: [], hidden: [finding] }))
    await waitFor(() =>
      expect(tabs.getByRole('button', { name: /^Properties/ })).toHaveAttribute(
        'aria-pressed',
        'true',
      ),
    )
    expect(tabs.queryByRole('button', { name: /^QC/ })).not.toBeInTheDocument()
  })

  it('debounces edits, observes terminal jobs, settings and agent completion without manual checks', async () => {
    setup()
    mount(<Auto />)
    await waitFor(() => expect(commands.getChapterContext).toHaveBeenCalled())
    act(() => queryClient.setQueryData(projectKey, { ...project, revision: 2 }))
    await new Promise((resolve) => setTimeout(resolve, 300))
    expect(commands.runQc).not.toHaveBeenCalled()
    await waitFor(() => expect(commands.runQc).toHaveBeenCalledOnce(), { timeout: 1800 })
    expect(commands.runQc).toHaveBeenLastCalledWith({ scope: 'project' }, expect.anything(), true)
    act(() =>
      useKoharuStore.setState({
        jobs: {
          pipeline: {
            id: 'pipeline',
            state: 'finished_with_problems',
            completed: 1,
            total: 2,
            page: null,
            stage: null,
            model: null,
            error: null,
            retry: null,
            failures: [],
            project: null,
            operation: null,
            finish_sound: false,
          },
        },
      }),
    )
    await waitFor(() => expect(commands.runQc).toHaveBeenCalledTimes(2), { timeout: 1800 })
    act(() => window.dispatchEvent(new Event(historyChangedEvent)))
    await waitFor(() => expect(commands.runQc).toHaveBeenCalledTimes(3), { timeout: 1800 })
    act(() => queryClient.setQueryData(qcSettingsKey, { min_font_size: 16 }))
    await waitFor(() => expect(commands.runQc).toHaveBeenCalledTimes(4), { timeout: 1800 })
  })
  it('validates and saves the minimum font size in preferences', async () => {
    setup()
    mount(<QcPreferences />)
    const minimum = await screen.findByRole('spinbutton')
    await waitFor(() => expect(minimum).toBeEnabled())
    fireEvent.change(minimum, { target: { value: '0' } })
    fireEvent.blur(minimum)
    expect(commands.saveQcSettings).not.toHaveBeenCalled()
    expect(minimum).toHaveValue(12)
    fireEvent.change(minimum, { target: { value: '16' } })
    fireEvent.blur(minimum)
    await waitFor(() => expect(commands.saveQcSettings).toHaveBeenCalledWith({ min_font_size: 16 }))
    await waitFor(() =>
      expect(queryClient.getQueryData(qcSettingsKey)).toEqual({ min_font_size: 16 }),
    )
  })
})
