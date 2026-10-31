import { vi } from 'vitest'

import { usePrepDialog } from '@/components/raw-prep/data'
import { commands } from '@koharu/bridge/protocol'

export function mockWo17() {
  usePrepDialog.getState().close()
  vi.spyOn(commands, 'getRawPrepSettings').mockResolvedValue({
    options: { levels: true, clip_percent: 0.5, whiten: false, threshold: 235 },
    fix_on_import: false,
  })
  vi.spyOn(commands, 'saveRawPrepSettings').mockImplementation(async (settings) => settings)
  vi.spyOn(commands, 'getPrepPageStatus').mockResolvedValue([])
  vi.spyOn(commands, 'previewPrep').mockResolvedValue({
    before: [],
    after: [],
    black: 40,
    white: 200,
  })
  vi.spyOn(commands, 'preparePages').mockResolvedValue({ changed: 0, skipped: [] })
  vi.spyOn(commands, 'restoreOriginalPages').mockResolvedValue({ changed: 0, skipped: [] })
}
