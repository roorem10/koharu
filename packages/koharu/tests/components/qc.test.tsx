import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { QcPanel } from '@/components/qc/QcPanel'
import { pageKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Page, type ProjectInfo, type QcFinding } from '@koharu/bridge/protocol'

vi.mock('@tauri-apps/api/core', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@tauri-apps/api/core')>()),
  Channel: class<T> {
    onmessage = (_value: T) => {}
  },
}))

vi.mock('@koharu/bridge/canvas', () => ({ showCanvasPage: vi.fn(() => false) }))

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

function open() {
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pageKey, page)
  vi.mocked(commands.runQc).mockResolvedValue({
    revision: 1,
    findings: [finding],
    hidden: [],
    pages_checked: 2,
  })
  render(
    <QueryClientProvider client={queryClient}>
      <QcPanel />
    </QueryClientProvider>,
  )
}

async function run() {
  const button = await screen.findByRole('button', { name: 'Check chapter' })
  await waitFor(() => expect(button).toBeEnabled())
  fireEvent.click(button)
  await screen.findByText('Hello 世界')
}

describe('QC review', () => {
  it('opens a finding with the shared page selection and box selection state', async () => {
    open()
    const next = { ...page, id: 'page-2', label: '02.png' }
    vi.spyOn(commands, 'selectPage').mockResolvedValue({
      project: { ...project, active_page: 'page-2' },
      page: next,
    })
    await run()
    fireEvent.click(screen.getByRole('button', { name: /Japanese remains.*Hello 世界/ }))
    await waitFor(() => expect(useKoharuStore.getState().selectedLayers).toEqual(['box-2']))
    expect(commands.selectPage).toHaveBeenCalledWith('page-2')
    expect(useKoharuStore.getState().selectedPages).toEqual(['page-2'])
    expect(queryClient.getQueryData(pageKey)).toEqual(next)
    expect(queryClient.getQueryData<ProjectInfo>(projectKey)?.active_page).toBe('page-2')
  })

  it('persists OK, reveals saved hidden findings on another check, and supports Undo OK', async () => {
    open()
    await run()
    fireEvent.click(screen.getByRole('button', { name: /Mark Japanese remains.*OK/ }))
    await waitFor(() => expect(screen.queryByText('Hello 世界')).not.toBeInTheDocument())
    expect(commands.markQcOk).toHaveBeenCalledWith('box-2', 'leftover_source', true)
    vi.mocked(commands.runQc).mockResolvedValue({
      revision: 1,
      findings: [],
      hidden: [finding],
      pages_checked: 2,
    })
    fireEvent.click(screen.getByRole('button', { name: 'Check chapter' }))
    await waitFor(() => expect(commands.runQc).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(screen.getByRole('button', { name: 'Check chapter' })).toBeEnabled())
    fireEvent.click(screen.getByRole('checkbox', { name: 'Show hidden' }))
    await screen.findByText('Hello 世界')
    fireEvent.click(screen.getByRole('button', { name: /Undo OK for Japanese remains/ }))
    await waitFor(() =>
      expect(commands.markQcOk).toHaveBeenLastCalledWith('box-2', 'leftover_source', false),
    )
    await waitFor(() =>
      expect(screen.getByRole('button', { name: /Mark Japanese remains.*OK/ })).toBeEnabled(),
    )
  })

  it('keeps the previous results after edits and warns about their revision', async () => {
    open()
    await run()
    act(() => queryClient.setQueryData(projectKey, { ...project, revision: 2 }))
    expect(await screen.findByText('Results may be out of date')).toBeInTheDocument()
    expect(screen.getByText('Hello 世界')).toBeInTheDocument()
    expect(commands.runQc).toHaveBeenCalledOnce()
  })

  it('dismisses the matching current flag rather than acknowledging or using an old index', async () => {
    open()
    const flag = { kind: 'review', message: 'Check the joke', page: 2 } as const
    vi.mocked(commands.runQc).mockResolvedValue({
      revision: 1,
      findings: [{ ...finding, check: 'flag', detail: flag.message, flag }],
      hidden: [],
      pages_checked: 2,
    })
    vi.spyOn(commands, 'getChapterContext').mockResolvedValue({
      context: {
        version: 1,
        series: null,
        brief: null,
        suggestions: [],
        lines: { 'box-2': { flags: [{ kind: 'review', message: 'Another flag', page: 2 }, flag] } },
      },
      series: null,
      supports_tasks: false,
    })
    vi.spyOn(commands, 'dismissLineFlag').mockResolvedValue(null)
    await run()
    fireEvent.click(screen.getByRole('button', { name: 'Mark Check the joke OK' }))
    await waitFor(() => expect(commands.dismissLineFlag).toHaveBeenCalledWith('box-2', 1))
    expect(commands.markQcOk).not.toHaveBeenCalled()
    await waitFor(() => expect(screen.queryByText('Check the joke')).not.toBeInTheDocument())
  })

  it('disables the panel during a check and consumes per-page progress', async () => {
    open()
    let finish!: (value: Awaited<ReturnType<typeof commands.runQc>>) => void
    vi.mocked(commands.runQc).mockImplementation((_scope, channel) => {
      channel.onmessage({ completed: 1, total: 40 })
      return new Promise((resolve) => {
        finish = resolve
      })
    })
    await waitFor(() => expect(screen.getByRole('button', { name: 'Check chapter' })).toBeEnabled())
    fireEvent.click(screen.getByRole('button', { name: 'Check chapter' }))
    await screen.findByText('Checking pages: 1 / 40')
    expect(screen.getByRole('button', { name: 'Check page' })).toBeDisabled()
    expect(screen.getByRole('checkbox', { name: 'Show hidden' })).toHaveAttribute(
      'aria-disabled',
      'true',
    )
    fireEvent.click(screen.getByRole('checkbox', { name: 'Show hidden' }))
    expect(screen.getByRole('checkbox', { name: 'Show hidden' })).toHaveAttribute(
      'aria-checked',
      'false',
    )
    expect(screen.getByRole('spinbutton')).toBeDisabled()
    await act(async () => finish({ revision: 1, findings: [], hidden: [], pages_checked: 40 }))
    await waitFor(() => expect(screen.getByRole('button', { name: 'Check chapter' })).toBeEnabled())
  })
})
