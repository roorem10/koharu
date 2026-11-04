import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { McpPreferences } from '@/components/preferences/McpPreferences'
import { queryClient } from '@/lib/queries'
import { commands, type McpStatus } from '@koharu/bridge/protocol'

function mount() {
  return render(
    <QueryClientProvider client={queryClient}>
      <McpPreferences />
    </QueryClientProvider>,
  )
}

function status(change: Partial<McpStatus> = {}): McpStatus {
  return {
    settings: { enabled: true, port: 48999, token: 'private-test-token', allow_edits: true },
    endpoint: 'http://127.0.0.1:48999/mcp',
    running: true,
    error: null,
    ...change,
  }
}

describe('MCP preferences', () => {
  it('shows the running status and copies the real token and connection details', async () => {
    const saved = status()
    vi.mocked(commands.getMcpStatus).mockResolvedValue(saved)
    const user = userEvent.setup()
    const copy = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue()
    mount()
    await screen.findByText(`Running at ${saved.endpoint}`)
    await user.click(screen.getByRole('button', { name: 'Copy access token' }))
    expect(copy).toHaveBeenLastCalledWith(saved.settings.token)
    const snippet = screen.getByRole('textbox', { name: 'Claude Code' })
    expect(snippet).toHaveValue(
      `claude mcp add --transport http booharu ${saved.endpoint} --header "Authorization: Bearer ${saved.settings.token}"`,
    )
    await user.click(screen.getByRole('button', { name: 'Copy Claude Code connection details' }))
    expect(copy).toHaveBeenLastCalledWith((snippet as HTMLTextAreaElement).value)
    expect(screen.getByRole('textbox', { name: 'Apps using a stdio server' })).toHaveValue(
      `npx mcp-remote ${saved.endpoint} --header "Authorization: Bearer ${saved.settings.token}"`,
    )
  })

  it('applies a valid port without saving each keystroke and prevents port zero', async () => {
    const user = userEvent.setup()
    mount()
    await waitFor(() => expect(screen.getByRole('spinbutton', { name: 'Port' })).toBeEnabled())
    const port = screen.getByRole('spinbutton', { name: 'Port' })
    await user.clear(port)
    await user.type(port, '0')
    expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled()
    expect(commands.saveMcpSettings).not.toHaveBeenCalled()
    await user.clear(port)
    await user.type(port, '48999')
    expect(commands.saveMcpSettings).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: 'Apply' }))
    await waitFor(() =>
      expect(commands.saveMcpSettings).toHaveBeenLastCalledWith({
        enabled: false,
        port: 48999,
        allow_edits: true,
      }),
    )
    await waitFor(() =>
      expect(
        (screen.getByRole('textbox', { name: 'Claude Code' }) as HTMLTextAreaElement).value,
      ).toContain('127.0.0.1:48999/mcp'),
    )
  })

  it('rotates the token and toggles editing while the server is off', async () => {
    const user = userEvent.setup()
    mount()
    await screen.findByRole('textbox', { name: 'Access token' })
    const button = screen.getByRole('button', { name: 'New token' })
    await waitFor(() => expect(button).toBeEnabled())
    await user.click(button)
    await waitFor(() =>
      expect(screen.getByRole('textbox', { name: 'Access token' })).toHaveValue('new-test-token'),
    )
    const edit = screen.getByRole('switch', { name: 'Allow edits' })
    await waitFor(() => expect(edit).toBeEnabled())
    await user.click(edit)
    await waitFor(() =>
      expect(commands.saveMcpSettings).toHaveBeenLastCalledWith({
        enabled: false,
        port: 47821,
        allow_edits: false,
      }),
    )
    expect(commands.newMcpToken).toHaveBeenCalledOnce()
  })

  it('shows a bind failure and lets the user turn the server off', async () => {
    const saved = status({ running: false, error: 'Port is already in use' })
    vi.mocked(commands.getMcpStatus).mockResolvedValue(saved)
    const user = userEvent.setup()
    mount()
    await screen.findByText('Server error: Port is already in use')
    await user.click(
      screen.getByRole('switch', { name: 'Let other AI apps use Booharu (MCP server)' }),
    )
    await waitFor(() =>
      expect(commands.saveMcpSettings).toHaveBeenLastCalledWith({
        enabled: false,
        port: 48999,
        allow_edits: true,
      }),
    )
  })
})
