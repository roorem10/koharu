import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

export function mockWo14() {
  vi.spyOn(commands, 'getRunSettings').mockResolvedValue({
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
