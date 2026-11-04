import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

// WO-11C owns this file.
export function mockWo11c() {
  vi.spyOn(commands, 'getTextGlyphBoxes').mockResolvedValue([])
}
