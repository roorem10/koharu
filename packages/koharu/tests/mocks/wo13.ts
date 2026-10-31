import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

export function mockWo13() {
  vi.spyOn(commands, 'getLibrarySettings').mockResolvedValue({
    export_name_pattern: '{index}_{page}',
  })
  vi.spyOn(commands, 'saveLibrarySettings').mockImplementation(async (settings) => settings)
  vi.spyOn(commands, 'exportSelectedPages').mockResolvedValue(null)
}
