import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

export function mockWo37() {
  vi.spyOn(commands, 'getPageCheckResults').mockResolvedValue({ revision: 1, pages: [] })
  vi.spyOn(commands, 'startPageCheck').mockResolvedValue('page-check-job')
  vi.spyOn(commands, 'applyPageCheckFix').mockResolvedValue(true)
}
