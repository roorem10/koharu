import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { CheckCleanupControl } from '@/components/cleanup/CheckCleanupControl'
import { useCleanupFindingFocus, useFocusCleanupFinding } from '@/components/cleanup/findingFocus'
import { LeftoverOverlay } from '@/components/cleanup/LeftoverOverlay'
import { cleanupSettingsKey, defaultCleanupSettings } from '@/components/cleanup/settings'
import { CleanupPreferences } from '@/components/preferences/CleanupPreferences'
import { useCompareStore } from '@/components/qc/compare'
import { navigateFinding, qcKey } from '@/components/qc/data'
import { FindingRow } from '@/components/qc/FindingRow'
import { pageKey, pagesKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import translations from '@/public/locales/en-US/translation.json'
import {
  commands,
  type Job,
  type Page,
  type ProjectInfo,
  type QcFinding,
} from '@koharu/bridge/protocol'

vi.mock('@koharu/bridge/canvas', () => ({ showCanvasPage: vi.fn(() => false) }))

const project: ProjectInfo = {
  name: 'Book',
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
const bounds = { x: 50, y: 100, width: 80, height: 40 }
const finding: QcFinding = {
  page: 'page-2',
  page_label: '02.png',
  layer: 'cleanup-2',
  check: 'leftover_cleanup',
  bounds,
  detail: '',
  translation: '',
  flag: null,
}
const running: Job = {
  id: 'check',
  state: 'running',
  completed: 0,
  total: 1,
  page: page.id,
  stage: 'detection',
  model: null,
  error: null,
  retry: null,
  failures: [],
  project: project.name,
  operation: { operation: 'check_cleanup' },
  finish_sound: false,
  auto_run: null,
}

function wrapper({ children }: { children: React.ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
}

beforeEach(() => {
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pageKey, page)
  queryClient.setQueryData(pagesKey, [{ ...page }, { ...page, id: 'page-2', label: '02.png' }])
  queryClient.setQueryData(cleanupSettingsKey, { ...defaultCleanupSettings })
})

describe('cleanup checking', () => {
  it('submits current or all pages through one job command and disables submission during a job', async () => {
    render(<CheckCleanupControl />, { wrapper })
    fireEvent.click(screen.getByRole('button', { name: translations.cleanup.check }))
    fireEvent.click(await screen.findByRole('menuitem', { name: translations.cleanup.checkPage }))
    await waitFor(() => expect(commands.checkCleanup).toHaveBeenCalledWith(['page-1']))
    await waitFor(() =>
      expect(screen.getByRole('button', { name: translations.cleanup.check })).toBeEnabled(),
    )
    fireEvent.click(screen.getByRole('button', { name: translations.cleanup.check }))
    fireEvent.click(await screen.findByRole('menuitem', { name: translations.cleanup.checkAll }))
    await waitFor(() => expect(commands.checkCleanup).toHaveBeenLastCalledWith(null))
    act(() => useKoharuStore.setState({ jobs: { check: running } }))
    expect(screen.getByRole('button', { name: translations.cleanup.check })).toBeDisabled()
  })

  it('refreshes stored decisions when a check job ends', async () => {
    render(<CheckCleanupControl />, { wrapper })
    await waitFor(() => expect(commands.getCleanupCheckResults).toHaveBeenCalledOnce())
    vi.mocked(commands.getCleanupCheckResults).mockResolvedValue([
      { page: page.id, layer: 'cleanup-1', findings: [], retried: true, protected: false },
    ])
    act(() => useKoharuStore.setState({ jobs: { check: { ...running, state: 'finished' } } }))
    await waitFor(() => expect(commands.getCleanupCheckResults).toHaveBeenCalledTimes(2))
    fireEvent.click(screen.getByRole('button', { name: translations.cleanup.check }))
    await screen.findByText(translations.cleanup.retryReason)
    expect(screen.getByText(translations.cleanup.clearResult)).toBeInTheDocument()
  })

  it('navigates with shared selection, exits comparison and selects the cleanup brush', async () => {
    const next = { ...page, id: finding.page, label: finding.page_label }
    vi.spyOn(commands, 'selectPage').mockResolvedValue({
      project: { ...project, active_page: next.id },
      page: next,
    })
    useCompareStore.setState({ pinned: true })
    await navigateFinding(finding)
    expect(useKoharuStore.getState()).toMatchObject({
      selectedPages: [finding.page],
      selectedLayers: [finding.layer],
      tool: 'remove',
    })
    expect(queryClient.getQueryData(pageKey)).toEqual(next)
    expect(useCompareStore.getState()).toMatchObject({ pinned: false, held: false })
    expect(useCleanupFindingFocus.getState().request).toEqual({
      project: project.name,
      page: finding.page,
      bounds,
    })
    render(<FindingRow finding={finding} />, { wrapper })
    expect(screen.getAllByRole('button')).toHaveLength(1)
  })

  it('waits for the requested prepared page before centering its bounds', () => {
    const element = document.createElement('div')
    vi.spyOn(element, 'getBoundingClientRect').mockReturnValue({
      width: 800,
      height: 600,
    } as DOMRect)
    const surface = { current: element }
    queryClient.setQueryData(projectKey, { ...project, active_page: finding.page })
    useCleanupFindingFocus.setState({
      request: { project: project.name, page: finding.page, bounds },
    })
    const { rerender } = renderHook(
      ({ ready, visiblePage }) => useFocusCleanupFinding(surface, project.name, visiblePage, ready),
      { initialProps: { ready: false, visiblePage: page.id } },
    )
    expect(useKoharuStore.getState().camera.fitted).toBe(true)
    expect(useCleanupFindingFocus.getState().request).not.toBeNull()
    rerender({ ready: true, visiblePage: finding.page })
    const camera = useKoharuStore.getState().camera
    expect(camera.fitted).toBe(false)
    expect(camera.translation[0] + (bounds.x + bounds.width / 2) * camera.zoom).toBeCloseTo(
      400 * window.devicePixelRatio,
    )
    expect(camera.translation[1] + (bounds.y + bounds.height / 2) * camera.zoom).toBeCloseTo(
      300 * window.devicePixelRatio,
    )
    expect(useCleanupFindingFocus.getState().request).toBeNull()
  })

  it('outlines only open leftover findings on the current page', () => {
    queryClient.setQueryData(qcKey(project.name), {
      revision: 1,
      pages_checked: 2,
      findings: [
        finding,
        { ...finding, page: page.id },
        { ...finding, page: page.id, check: 'overflow' },
      ],
      hidden: [{ ...finding, page: page.id }],
    })
    render(<LeftoverOverlay />, { wrapper })
    const rectangles = screen
      .getByTestId('cleanup-leftovers')
      .querySelectorAll('rect[vector-effect]')
    expect(rectangles).toHaveLength(1)
    expect(rectangles[0]).toHaveAttribute('x', '50')
    expect(rectangles[0]).toHaveAttribute('width', '80')
  })

  it('saves a separate retry model and can return to following the normal model', async () => {
    const user = userEvent.setup()
    render(<CleanupPreferences />, { wrapper })
    await user.click(screen.getByRole('combobox', { name: translations.cleanup.secondTryModel }))
    await user.click(await screen.findByRole('option', { name: 'AOT Inpainting' }))
    await waitFor(() =>
      expect(commands.saveCleanupSettings).toHaveBeenCalledWith({
        ...defaultCleanupSettings,
        second_try_model: { model: 'aot-inpainting' },
      }),
    )
    await waitFor(() =>
      expect(
        screen.getByRole('combobox', { name: translations.cleanup.secondTryModel }),
      ).toBeEnabled(),
    )
    await user.click(screen.getByRole('combobox', { name: translations.cleanup.secondTryModel }))
    await user.click(await screen.findByRole('option', { name: translations.cleanup.sameModel }))
    await waitFor(() =>
      expect(commands.saveCleanupSettings).toHaveBeenLastCalledWith(defaultCleanupSettings),
    )
  })
})
