import { vi } from 'vitest'

import { useCompareStore } from '@/components/qc/compare'
import { commands } from '@koharu/bridge/protocol'

export function mockWo12() {
  vi.spyOn(commands, 'getQcReview').mockResolvedValue([])
  vi.spyOn(commands, 'getQcFixHistory').mockResolvedValue([])
  vi.spyOn(commands, 'getQcFixSummary').mockResolvedValue({
    fixes_made: 0,
    items_left: 0,
    requests_used: 0,
  })
  vi.spyOn(commands, 'fixQc').mockResolvedValue('qc-fix-job')
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
