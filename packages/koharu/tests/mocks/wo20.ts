import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

export function mockWo20() {
  vi.spyOn(commands, 'getBoxActions').mockResolvedValue({ can_split: false, bubble_boxes: 0 })
  vi.spyOn(commands, 'getBoxSettings').mockResolvedValue({ join_in_bubble: true })
  vi.spyOn(commands, 'saveBoxSettings').mockImplementation(async (settings) => settings)
}
