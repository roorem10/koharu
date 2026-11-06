import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

export function mockWo14() {
  vi.spyOn(commands, 'getRunSettings').mockResolvedValue({
    auto_run_quality: 'balanced',
    auto_run_fix_levels: false,
    auto_run_export: false,
    retries: 2,
    on_page_failure: 'continue_with_other_pages',
    order: 'overlapped',
    finish_sound: true,
    presets: [
      { name: 'Detect + OCR', stages: ['detection', 'ocr'] },
      { name: 'Translate + clean up', stages: ['translation', 'inpainting'] },
    ],
  })
}
