import { vi } from 'vitest'

import { emptyEmphasisStyle } from '@/components/emphasis/settings'
import { commands, type EmphasisPreferences as EmphasisSettings } from '@koharu/bridge/protocol'

export function emphasisDefaults(): EmphasisSettings {
  return {
    enabled_for_ai: true,
    default: { ...emptyEmphasisStyle, font_weight: 700, size_scale: 1.15 },
    categories: {},
  }
}

export function mockWo11f() {
  vi.spyOn(commands, 'applyEmphasis').mockResolvedValue(null)
  vi.spyOn(commands, 'getEmphasisSettings').mockResolvedValue(emphasisDefaults())
  vi.spyOn(commands, 'saveEmphasisSettings').mockImplementation(async (settings) => settings)
}
