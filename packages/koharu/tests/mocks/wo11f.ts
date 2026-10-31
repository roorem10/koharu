import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

// WO-11F owns this file.
export function mockWo11f() {
  vi.spyOn(commands, 'applyEmphasis').mockResolvedValue(null)
}
