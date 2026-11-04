import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

export function mockWo06() {
  vi.spyOn(commands, 'getBoxNote').mockResolvedValue(null)
  vi.spyOn(commands, 'setBoxNote').mockResolvedValue(null)
}
