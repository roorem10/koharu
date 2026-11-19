import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, renderHook, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ActivityCenter } from '@/components/editor/ActivityCenter'
import { CanvasCommandBar } from '@/components/editor/CanvasCommandBar'
import { receiveBooharuJob, useBooharuJobEvents, useBooharuJobs } from '@/lib/booharu/booharuJobs'
import { queryClient } from '@/lib/queries'
import { commands, events, type BooharuJob } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { openChapter } from './jobs'
import { settingsDesktop } from './settings'

/** A read of the chapter by hand, going, with `job` its other values. */
const read = (job: Partial<BooharuJob> = {}): BooharuJob => ({
  id: 1,
  kind: 'read_chapter',
  state: 'running',
  message: null,
  step: 0,
  steps: 0,
  group: null,
  summary: null,
  ...job,
})

/** Tells `job`, as booharu_job does. */
const tell = (job: BooharuJob) => act(() => receiveBooharuJob(job))

function renderWithQueries(ui: React.ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>)
}

beforeEach(() => {
  useBooharuJobs.setState({ jobs: {} })
})

describe('Booharu’s reads in the Activity center', () => {
  it('lists a read going as Reading chapter, with its share done and Stop, until it ends', async () => {
    settingsDesktop()
    const user = userEvent.setup()
    renderWithQueries(<ActivityCenter />)
    expect(screen.queryByRole('complementary', { name: 'Activity' })).toBeNull()

    tell(read())
    expect(screen.getByRole('complementary', { name: 'Activity' })).toBeInTheDocument()
    expect(screen.getByText('Reading chapter')).toBeInTheDocument()
    // It shows going before it knows how many requests the chapter takes.
    expect(screen.getByRole('progressbar', { name: 'Reading chapter' })).not.toHaveAttribute(
      'aria-valuenow',
    )

    tell(read({ step: 1, steps: 4 }))
    expect(screen.getByRole('progressbar', { name: 'Reading chapter' })).toHaveAttribute(
      'aria-valuenow',
      '25',
    )
    expect(screen.getByText('25%')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Stop' }))
    expect(commands.stopPipeline).toHaveBeenCalledTimes(1)

    tell(read({ state: 'stopped', step: 1, steps: 4 }))
    expect(screen.queryByText('Reading chapter')).toBeNull()
    expect(screen.queryByRole('complementary', { name: 'Activity' })).toBeNull()
  })

  it('shows Stop on the Run button while a read goes, which stops it', async () => {
    settingsDesktop()
    openChapter('Chapter 1')
    const user = userEvent.setup()
    renderWithQueries(<CanvasCommandBar />)
    expect(screen.getByRole('button', { name: 'Run processing' })).toBeInTheDocument()

    tell(read({ group: 2, step: 1, steps: 3 }))
    await user.click(screen.getByRole('button', { name: 'Stop processing' }))
    expect(commands.stopPipeline).toHaveBeenCalledTimes(1)
    expect(commands.booharuRun).not.toHaveBeenCalled()

    tell(read({ group: 2, state: 'done', step: 3, steps: 3 }))
    expect(screen.getByRole('button', { name: 'Run processing' })).toBeInTheDocument()
  })
})

describe('Booharu’s jobs', () => {
  it('starts from the reads going as it loads, leaving out one told meanwhile', async () => {
    settingsDesktop()
    let handler: ((job: BooharuJob) => void) | undefined
    const stop = vi.fn()
    vi.spyOn(events, 'booharuJob').mockImplementation(async (listen) => {
      handler = listen
      return stop
    })
    let answer: (going: BooharuJob[]) => void = () => undefined
    vi.spyOn(commands, 'booharuGetJobs').mockReturnValue(
      new Promise((resolve) => {
        answer = resolve
      }),
    )
    const hook = renderHook(() => useBooharuJobEvents())
    await waitFor(() => expect(commands.booharuGetJobs).toHaveBeenCalledTimes(1))

    // Read 3 ends while the reads going load, which still name it.
    act(() => handler?.(read({ id: 3, state: 'done' })))
    await act(async () => answer([read({ id: 3, step: 1, steps: 2 }), read({ id: 4 })]))
    expect(Object.keys(useBooharuJobs.getState().jobs)).toEqual(['4'])

    hook.unmount()
    await waitFor(() => expect(stop).toHaveBeenCalledTimes(1))
  })

  it('says why a read a Run press started failed or was skipped, and nothing else', () => {
    const shown = vi.spyOn(toast, 'add')
    const why = "There's no text to read; run OCR first."

    tell(read({ id: 1, group: 5 }))
    tell(read({ id: 1, group: 5, state: 'skipped', message: why }))
    tell(read({ id: 2, group: 5, state: 'failed', message: 'no connection' }))
    // A read by hand answers through its command, and one stopped or done needs no word.
    tell(read({ id: 3, state: 'failed', message: 'no connection' }))
    tell(read({ id: 4, group: 5, state: 'stopped' }))
    tell(read({ id: 5, group: 5, state: 'done' }))

    expect(
      shown.mock.calls.map(([options]) => [options.type, options.title, options.description]),
    ).toEqual([
      ['warning', 'Read chapter was skipped', why],
      ['warning', 'Read chapter failed', 'no connection'],
    ])
    expect(useBooharuJobs.getState().jobs).toEqual({})
  })
})
