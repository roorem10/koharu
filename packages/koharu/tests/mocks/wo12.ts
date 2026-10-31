import { vi } from 'vitest'

import { useCompareStore } from '@/components/qc/compare'
import { commands } from '@koharu/bridge/protocol'

export function mockWo12() {
  useCompareStore.getState().reset()
  vi.spyOn(commands, 'getQcSettings').mockResolvedValue({ min_font_size: 12 })
  vi.spyOn(commands, 'saveQcSettings').mockImplementation(async (settings) => settings)
  vi.spyOn(commands, 'runQc').mockResolvedValue({
    revision: 1,
    findings: [],
    hidden: [],
    pages_checked: 1,
  })
  vi.spyOn(commands, 'markQcOk').mockResolvedValue(null)
  vi.spyOn(commands, 'getPageSource').mockResolvedValue([])
}
