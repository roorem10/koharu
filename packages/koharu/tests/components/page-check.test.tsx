import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { RightSidebar } from '@/components/editor/RightSidebar'
import { usePageCheckFocus } from '@/components/qc/page-check-data'
import { PageCheckButton } from '@/components/qc/PageCheckButton'
import { PageCheckOverlay } from '@/components/qc/PageCheckOverlay'
import { PageCheckResults } from '@/components/qc/PageCheckResults'
import { pageKey, pagesKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import translations from '@/public/locales/en-US/translation.json'
import { commands, type Page, type PageCheckPage, type ProjectInfo } from '@koharu/bridge/protocol'

vi.mock('@koharu/bridge/canvas', () => ({ showCanvasPage: vi.fn(() => false) }))
vi.mock('@/components/editor/Inspector', () => ({ Inspector: () => <div /> }))
vi.mock('@/components/editor/AgentPanel', () => ({ AgentPanel: () => <div /> }))

const labels = translations.pageCheck
const project: ProjectInfo = {
  name: 'Page check test',
  revision: 8,
  active_page: 'one',
  can_undo: false,
  can_redo: false,
}
const page: Page = {
  id: 'one',
  label: '01.png',
  size: { width: 1000, height: 1400 },
  layers: [],
  regions: [],
}
const reportPage: PageCheckPage = {
  page: 'two',
  page_label: '02.png',
  revision: 8,
  status: 'checked',
  reason: null,
  findings: [
    {
      id: 'unmatched',
      kind: 'missed_text',
      detail: 'Small text near the left edge',
      area: { x: 20, y: 400, width: 90, height: 200 },
      matched_layer: null,
      fix: null,
    },
    {
      id: 'outline',
      kind: 'hard_to_read',
      detail: 'Text blends into the dark background',
      area: { x: 200, y: 300, width: 100, height: 60 },
      matched_layer: 'layer-two',
      fix: 'outline',
    },
  ],
}

function mount(element: React.ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{element}</QueryClientProvider>)
}

beforeEach(() => {
  usePageCheckFocus.setState({ focus: null })
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pageKey, page)
  queryClient.setQueryData(pagesKey, [
    { id: 'one', label: '01.png' },
    { id: 'two', label: '02.png' },
  ])
  vi.mocked(commands.getPageCheckResults).mockResolvedValue({
    revision: 8,
    pages: [structuredClone(reportPage)],
  })
})

describe('page check', () => {
  it('does not run until confirmed and submits the chosen page scope', async () => {
    mount(<PageCheckButton />)
    fireEvent.click(screen.getByRole('button', { name: labels.checkPage }))
    expect(commands.startPageCheck).not.toHaveBeenCalled()
    const dialog = within(screen.getByRole('dialog'))
    fireEvent.click(dialog.getByRole('button', { name: translations.common.cancel }))
    expect(commands.startPageCheck).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: labels.checkPage }))
    fireEvent.click(screen.getByRole('button', { name: labels.start }))
    await waitFor(() => expect(commands.startPageCheck).toHaveBeenCalledWith(['one']))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    fireEvent.click(screen.getByRole('button', { name: labels.checkPage }))
    fireEvent.click(screen.getByRole('button', { name: labels.allPages }))
    expect(screen.getByRole('button', { name: labels.allPages })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    fireEvent.click(screen.getByRole('button', { name: labels.start }))
    await waitFor(() => expect(commands.startPageCheck).toHaveBeenLastCalledWith([]))
  })

  it('navigates an unmatched area through the shared page selection and outlines it without selecting a fabricated layer', async () => {
    vi.spyOn(commands, 'selectPage').mockResolvedValue({
      project: { ...project, active_page: 'two' },
      page: { ...page, id: 'two' },
    })
    const bounds = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: 800,
      bottom: 600,
      width: 800,
      height: 600,
      toJSON: () => ({}),
    })
    mount(
      <>
        <PageCheckResults />
        <PageCheckOverlay page='two' />
      </>,
    )
    fireEvent.click(
      await screen.findByRole('button', { name: new RegExp(reportPage.findings[0].detail) }),
    )
    await waitFor(() => expect(useKoharuStore.getState().selectedPages).toEqual(['two']))
    await waitFor(() => expect(screen.getByTestId('page-check-area')).toBeInTheDocument())
    expect(commands.selectPage).toHaveBeenCalledWith('two')
    expect(useKoharuStore.getState().selectedLayers).toEqual([])
    expect(usePageCheckFocus.getState().focus?.area).toEqual(reportPage.findings[0].area)
    expect(useKoharuStore.getState().camera.fitted).toBe(false)
    expect(screen.getAllByRole('button', { name: labels.outline })).toHaveLength(1)
    act(() => queryClient.setQueryData(projectKey, { ...project, active_page: 'two', revision: 9 }))
    await waitFor(() => expect(screen.queryByTestId('page-check-area')).not.toBeInTheDocument())
    bounds.mockRestore()
  })

  it('allows the advertised concrete fix, then disables it after a revision change', async () => {
    mount(<PageCheckResults />)
    const fix = await screen.findByRole('button', { name: labels.outline })
    fireEvent.click(fix)
    await waitFor(() => expect(commands.applyPageCheckFix).toHaveBeenCalledWith('two', 'outline'))
    act(() => queryClient.setQueryData(projectKey, { ...project, revision: 9 }))
    await waitFor(() => expect(screen.getByRole('button', { name: labels.outline })).toBeDisabled())
    fireEvent.click(screen.getByRole('button', { name: labels.outline }))
    expect(commands.applyPageCheckFix).toHaveBeenCalledOnce()
  })

  it('keeps skipped reports reachable in QC without claiming the page passed', async () => {
    vi.mocked(commands.getPageCheckResults).mockResolvedValue({
      revision: 8,
      pages: [{ ...reportPage, status: 'skipped', reason: 'no_vision_model', findings: [] }],
    })
    mount(<RightSidebar />)
    const tabs = within(screen.getByRole('group', { name: translations.qc.editorPanels }))
    fireEvent.click(await tabs.findByRole('button', { name: `${translations.qc.title} 0` }))
    expect(await screen.findByText(labels.noVision)).toBeVisible()
    expect(screen.queryByText(labels.noFindings)).not.toBeInTheDocument()
    expect(screen.queryByText(translations.qc.noFindings)).not.toBeInTheDocument()
  })

  it('refreshes sidecar results when the ordinary job completes', async () => {
    vi.mocked(commands.getPageCheckResults).mockResolvedValue({ revision: 8, pages: [] })
    mount(<PageCheckResults />)
    await waitFor(() => expect(commands.getPageCheckResults).toHaveBeenCalledOnce())
    vi.mocked(commands.getPageCheckResults).mockResolvedValue({ revision: 8, pages: [reportPage] })
    act(() =>
      useKoharuStore.setState({
        jobs: {
          check: {
            id: 'check',
            state: 'finished',
            completed: 1,
            total: 1,
            page: 'two',
            stage: null,
            model: 'page_check',
            error: null,
            retry: null,
            failures: [],
            project: project.name,
            operation: null,
            finish_sound: false,
          },
        },
      }),
    )
    await screen.findByText(reportPage.findings[0].detail)
    expect(commands.getPageCheckResults).toHaveBeenCalledTimes(2)
  })

  it('keeps QC open while refreshing after edits, but does not show another project’s results', async () => {
    mount(<RightSidebar />)
    const tabs = within(screen.getByRole('group', { name: translations.qc.editorPanels }))
    fireEvent.click(await tabs.findByRole('button', { name: `${translations.qc.title} 2` }))
    vi.mocked(commands.getPageCheckResults).mockReturnValue(new Promise(() => {}))
    act(() => queryClient.setQueryData(projectKey, { ...project, revision: 9 }))
    await waitFor(() => expect(commands.getPageCheckResults).toHaveBeenCalledTimes(2))
    expect(tabs.getByRole('button', { name: `${translations.qc.title} 2` })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    expect(screen.getByRole('button', { name: labels.outline })).toBeDisabled()
    act(() => queryClient.setQueryData(projectKey, { ...project, name: 'Other project' }))
    await waitFor(() =>
      expect(screen.queryByText(reportPage.findings[0].detail)).not.toBeInTheDocument(),
    )
  })
})
