import { vi } from 'vitest'

import { useRedesignStore } from '@/components/redesign/state'
import { commands } from '@koharu/bridge/protocol'

export function mockWo23() {
  useRedesignStore.setState({ tab: 'text', popup: null })
  vi.spyOn(commands, 'getBoxEffects').mockImplementation(async (layers) => layers.map(() => false))
  vi.spyOn(commands, 'resetBoxEffects').mockResolvedValue(null)
}
