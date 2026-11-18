import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { CanvasCommandBar } from '@/components/editor/CanvasCommandBar'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type RunPlan } from '@koharu/ipc/protocol'
import { Toaster } from '@koharu/ui/components/toast'

import { every, openChapter } from './jobs'
import { settingsDesktop } from './settings'

/** The desktop, with booharu_run answering `plan`, and the editor's command bar on Chapter 1. */
function renderCommandBar(plan: RunPlan = { group: 1, groups: [], run: 3 }) {
  settingsDesktop()
  const carryOn = vi.spyOn(commands, 'booharuRun').mockResolvedValue(plan)
  const run = vi.spyOn(commands, 'runPipeline').mockResolvedValue(4)
  openChapter('Chapter 1')
  render(
    <QueryClientProvider client={queryClient}>
      <CanvasCommandBar />
      <Toaster />
    </QueryClientProvider>,
  )
  return { carryOn, run, user: userEvent.setup() }
}

const runButton = () => screen.getByRole('button', { name: 'Run processing' })

describe('Booharu’s Run button', () => {
  it('carries on with every stage on, on the project or the pages chosen', async () => {
    const { carryOn, run, user } = renderCommandBar()

    await user.click(runButton())
    await waitFor(() => expect(carryOn).toHaveBeenCalledWith({ kind: 'project' }, every))

    act(() =>
      useKoharuStore.setState({ processingScope: 'selected-pages', selectedPages: ['p3', 'p1'] }),
    )
    await user.click(runButton())
    await waitFor(() =>
      expect(carryOn).toHaveBeenLastCalledWith({ kind: 'pages', pages: ['p3', 'p1'] }, every),
    )
    expect(run).not.toHaveBeenCalled()
  })

  it('runs Koharu’s way with fewer stages on', async () => {
    const { carryOn, run, user } = renderCommandBar()
    act(() => useKoharuStore.setState({ processingStages: ['translate', 'inpaint'] }))

    await user.click(runButton())
    await waitFor(() => expect(run).toHaveBeenCalledWith('project', ['inpaint', 'translate']))
    expect(carryOn).not.toHaveBeenCalled()
  })

  it('says when the pages are already done, offering to run every stage again', async () => {
    const { carryOn, run, user } = renderCommandBar({ group: 2, groups: [], run: null })

    await user.click(runButton())
    expect(await screen.findByText('These pages are already done.')).toBeInTheDocument()
    expect(carryOn).toHaveBeenCalledTimes(1)
    expect(run).not.toHaveBeenCalled()

    await user.click(screen.getByRole('button', { name: 'Run all stages again' }))
    await waitFor(() => expect(run).toHaveBeenCalledWith('project', every))
  })

  it('readies the finish chime’s audio on its click', async () => {
    const started = vi.fn()
    vi.stubGlobal(
      'AudioContext',
      class {
        state = 'suspended'
        constructor() {
          started()
        }
        resume = vi.fn(async () => undefined)
      },
    )
    try {
      const { user } = renderCommandBar()
      expect(started).not.toHaveBeenCalled()
      await user.click(runButton())
      expect(started).toHaveBeenCalledTimes(1)
    } finally {
      vi.unstubAllGlobals()
    }
  })
})
