import { vi } from 'vitest'

import { commands, type McpStatus } from '@koharu/bridge/protocol'

export function mockWo21() {
  let status: McpStatus = {
    settings: { enabled: false, port: 47821, token: 'test-token', allow_edits: true },
    endpoint: 'http://127.0.0.1:47821/mcp',
    running: false,
    error: null,
  }
  vi.spyOn(commands, 'getMcpStatus').mockImplementation(async () => structuredClone(status))
  vi.spyOn(commands, 'saveMcpSettings').mockImplementation(async (options) => {
    status = {
      ...status,
      settings: { ...status.settings, ...options },
      endpoint: `http://127.0.0.1:${options.port}/mcp`,
      running: options.enabled,
    }
    return structuredClone(status)
  })
  vi.spyOn(commands, 'newMcpToken').mockImplementation(async () => {
    status = { ...status, settings: { ...status.settings, token: 'new-test-token' } }
    return structuredClone(status)
  })
}
