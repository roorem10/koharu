import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

// WO-11A owns this file. Defaults so other components can call the word-style commands.
export function mockWo11a() {
  vi.spyOn(commands, 'setWordStyle').mockResolvedValue(null)
  vi.spyOn(commands, 'clearWordStyles').mockResolvedValue(null)
  vi.spyOn(commands, 'replaceTextRange').mockResolvedValue(null)
  vi.spyOn(commands, 'getWordStyles').mockResolvedValue([])
}
