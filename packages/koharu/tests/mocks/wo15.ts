import { vi } from 'vitest'

import { useCleanupQueue } from '@/components/cleanup/queue'
import { defaultCleanupSettings } from '@/components/cleanup/settings'
import { commands } from '@koharu/bridge/protocol'

export function mockWo15() {
  vi.spyOn(commands, 'getCleanupSettings').mockResolvedValue({ ...defaultCleanupSettings })
  vi.spyOn(commands, 'saveCleanupSettings').mockImplementation(async (settings) => settings)
  vi.spyOn(commands, 'cleanAreas').mockResolvedValue('cleanup-job')
  useCleanupQueue.setState((state) => ({
    project: null,
    generation: state.generation + 1,
    nextId: 1,
    pages: {},
    pending: {},
    drawing: false,
  }))
}
