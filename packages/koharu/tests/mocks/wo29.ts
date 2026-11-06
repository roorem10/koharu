import { vi } from 'vitest'

import { useCleanupFindingFocus } from '@/components/cleanup/findingFocus'
import { commands } from '@koharu/bridge/protocol'

export function mockWo29() {
  vi.spyOn(commands, 'checkCleanup').mockResolvedValue('cleanup-check-job')
  vi.spyOn(commands, 'getCleanupCheckResults').mockResolvedValue([])
  useCleanupFindingFocus.setState({ request: null })
}
