'use client'

import { InferenceControl } from '@/components/editor/InferenceControl'
import { call } from '@/lib/backend'
import { runOrCarryOn } from '@/lib/booharu/runs' // Booharu: Run carries on with what's left.
import { usePage } from '@/lib/queries'
import { pipelineStages, useKoharuStore, type PipelineScope } from '@/lib/store'
import { commands, type Stage, type Target } from '@koharu/ipc/protocol'

export function CanvasCommandBar() {
  const page = usePage().data
  const selectedPages = useKoharuStore((state) => state.selectedPages)
  const jobs = useKoharuStore((state) => state.jobs)
  const running = Object.values(jobs).find((job) => job.state === 'running')

  const run = (selection: PipelineScope, stages: Stage[]) => {
    const target: Target = selection === 'project' ? 'project' : { pages: selectedPages }
    // A run takes its stages in the pipeline's order, whichever order they were picked in.
    const ordered = pipelineStages.filter((stage) => stages.includes(stage))
    // Booharu: with every stage on, Run carries on with what the pages still need.
    void runOrCarryOn(target, ordered, () => call(commands.runPipeline, target, ordered)).catch(
      () => undefined,
    )
  }

  return (
    <header className='flex h-10 shrink-0 items-center gap-2 border-b border-border/80 bg-[var(--surface-toolbar)] px-2.5'>
      <div className='min-w-0 flex-1' />
      <InferenceControl disabled={!page || Boolean(running)} onRun={run} />
    </header>
  )
}
