import { call } from '@/lib/backend'
import { commands, type TermUsage } from '@koharu/bridge/protocol'

/** The existing term-fix path deliberately preserves all manually translated lines. */
export async function retranslateTerm(usage: TermUsage): Promise<void> {
  if (usage.layers.length === usage.skipped) return
  await call(
    commands.process,
    { scope: 'entities', value: usage.layers },
    { operation: 'only', stage: 'translation' },
  )
}
