import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

export function mockWo10() {
  vi.spyOn(commands, 'getTextWarp').mockImplementation(async (layers) => layers.map(() => null))
  vi.spyOn(commands, 'setTextWarp').mockResolvedValue(null)
}
