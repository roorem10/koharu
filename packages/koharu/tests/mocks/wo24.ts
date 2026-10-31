import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

export function mockWo24() {
  vi.spyOn(commands, 'getReadingModel').mockResolvedValue({
    selection: { provider: 'openai', model: 'reader', vision: true },
    vision: true,
    local: false,
    label: 'OpenAI · reader',
  })
  vi.spyOn(commands, 'getReadingModels').mockResolvedValue([])
  vi.spyOn(commands, 'getLastReadModel').mockResolvedValue(null)
  vi.spyOn(commands, 'getContextSettings').mockResolvedValue({
    read_when: 'missing',
    send_images: true,
    pages_per_request: 20,
    previous_pages: 1,
    agent_messages: 20,
    ask_before_reading: true,
    read_model: null,
  })
  vi.spyOn(commands, 'saveContextSettings').mockImplementation(async (settings) => settings)
  vi.spyOn(commands, 'readChapter').mockResolvedValue('read-job')
}
