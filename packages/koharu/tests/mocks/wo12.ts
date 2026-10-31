import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

export function mockWo12() {
  vi.spyOn(commands, 'getQcSettings').mockResolvedValue({ min_font_size: 12 })
  vi.spyOn(commands, 'saveQcSettings').mockImplementation(async (settings) => settings)
  vi.spyOn(commands, 'runQc').mockResolvedValue({
    revision: 1,
    findings: [],
    hidden: [],
    pages_checked: 1,
  })
  vi.spyOn(commands, 'markQcOk').mockResolvedValue(null)
}
