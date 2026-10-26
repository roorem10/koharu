import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { SettingsPage } from '@/components/preferences/SettingsPage'
import { defaultShortcuts, useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/bridge/protocol'

vi.mock('@/lib/backend', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/backend')>()),
  refreshPreferences: vi.fn(async () => undefined),
  refreshTranslationModels: vi.fn(async () => undefined),
}))

beforeEach(() => {
  vi.spyOn(commands, 'saveShortcuts').mockResolvedValue({})
})

function open() {
  useKoharuStore.setState({ settingsOpen: true })
  const view = render(<SettingsPage />)
  fireEvent.click(screen.getByRole('button', { name: 'Shortcuts' }))
  return view
}

function record(action: string, options: KeyboardEventInit, alternate = false) {
  const button = screen.getByRole('button', {
    name: `${action} · ${alternate ? 'Alternate' : 'Primary'} shortcut`,
  })
  fireEvent.click(button)
  fireEvent.keyDown(button, options)
  return button
}

describe('shortcut preferences', () => {
  it('records two slots, keeps the old value on Escape, clears slots and resets defaults', async () => {
    open()
    record('Text', { key: 'Alt', altKey: true })
    expect(screen.getByText('Press keys…')).toBeInTheDocument()
    fireEvent.keyDown(screen.getByRole('button', { name: 'Text · Primary shortcut' }), {
      key: 'z',
      altKey: true,
    })
    await waitFor(() => expect(commands.saveShortcuts).toHaveBeenCalled())
    expect(useKoharuStore.getState().shortcuts.text).toBe('alt+z')
    record('Text', { key: 'F2' }, true)
    expect(useKoharuStore.getState().shortcuts.text_alt).toBe('f2')
    record('Text', { key: 'Escape' })
    expect(useKoharuStore.getState().shortcuts.text).toBe('alt+z')
    fireEvent.click(screen.getByRole('button', { name: 'Clear Text · Alternate shortcut' }))
    expect(useKoharuStore.getState().shortcuts.text_alt).toBe('')
    fireEvent.click(screen.getByRole('button', { name: 'Reset to defaults' }))
    expect(useKoharuStore.getState().shortcuts).toEqual(defaultShortcuts)
  })

  it('marks every conflicting row, blocks both exits and never saves a conflicting draft', async () => {
    open()
    record('Text', { key: 'v' })
    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent('Two shortcuts use the same keys')
    expect(screen.getByRole('button', { name: 'Text · Primary shortcut' })).toHaveAttribute(
      'aria-invalid',
      'true',
    )
    expect(screen.getByRole('button', { name: 'Select · Primary shortcut' })).toHaveAttribute(
      'aria-invalid',
      'true',
    )
    expect(commands.saveShortcuts).not.toHaveBeenCalled()
    expect(useKoharuStore.getState().shortcuts).toEqual(defaultShortcuts)
    fireEvent.click(screen.getByRole('button', { name: 'Back to editor' }))
    expect(useKoharuStore.getState().settingsOpen).toBe(true)
    expect(alert).toHaveFocus()
    expect(alert).toHaveAttribute('data-flash', 'true')
    fireEvent.click(screen.getByRole('button', { name: 'Appearance' }))
    expect(screen.getByRole('alert')).toBeInTheDocument()
    record('Text', { key: 'q' })
    await waitFor(() =>
      expect(commands.saveShortcuts).toHaveBeenCalledWith({ ...defaultShortcuts, text: 'q' }),
    )
    fireEvent.click(screen.getByRole('button', { name: 'Appearance' }))
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Back to editor' }))
    expect(useKoharuStore.getState().settingsOpen).toBe(false)
  })

  it('checks built-ins, drops conflicts on external closure, and shows fixed shortcuts without recorders', () => {
    open()
    record('Text', { key: 'z', ctrlKey: true })
    expect(screen.getByText('Used by Undo (built-in)')).toBeInTheDocument()
    expect(commands.saveShortcuts).not.toHaveBeenCalled()
    const fixed = screen.getByText('Built-in (can’t be changed)').closest('section')!
    expect(within(fixed).queryAllByRole('button')).toHaveLength(0)
    expect(within(fixed).getByText('Pan with the middle mouse button')).toBeInTheDocument()
    act(() => useKoharuStore.getState().setSettingsOpen(false))
    act(() => useKoharuStore.getState().setSettingsOpen(true))
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Text · Primary shortcut' })).toHaveTextContent('T')
    expect(useKoharuStore.getState().shortcuts).toEqual(defaultShortcuts)
  })
})
