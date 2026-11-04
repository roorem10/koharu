import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

export function mockWo07() {
  vi.spyOn(commands, 'getOcrBoxSettings').mockResolvedValue({
    default_mode: 'read_translate',
    subtitle_font_family: null,
    subtitle_size: 18,
    subtitle_fill: [0, 0, 0, 255],
    subtitle_outline: [255, 255, 255, 255],
  })
  vi.spyOn(commands, 'saveOcrBoxSettings').mockImplementation(async (settings) => settings)
  vi.spyOn(commands, 'getOcrRedoInfo').mockResolvedValue({ user_sources: 0, user_translations: 0 })
  vi.spyOn(commands, 'addOcrBox').mockResolvedValue('ocr-box')
  vi.spyOn(commands, 'redoOcrBoxes').mockResolvedValue(['job'])
  vi.spyOn(commands, 'addSfxSubtitle').mockResolvedValue('subtitle')
}
