import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { PresetShortcutDialog } from '@/components/editor/PresetShortcutDialog'
import { SettingsPage } from '@/components/preferences/SettingsPage'
import { defaultShortcuts, useKoharuStore } from '@/lib/store'
import { emptyTextStyle } from '@/lib/text-styles'
import { commands } from '@koharu/bridge/protocol'

vi.mock('@/lib/backend', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/backend')>()),
  refreshPreferences: vi.fn(async () => undefined),
  refreshTranslationModels: vi.fn(async () => undefined),
}))

beforeEach(() => {
  vi.spyOn(commands, 'saveShortcuts').mockResolvedValue({})
  vi.spyOn(commands, 'saveWheelZoom').mockImplementation(async (value) => value)
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
  it.each([
    [{ key: 't' }, 'Text'],
    [{ key: 'z', ctrlKey: true }, 'Undo'],
    [{ key: '1' }, 'Other preset'],
  ] as const)('refuses preset shortcut conflicts for %j', (event, owner) => {
    const save = vi.spyOn(commands, 'saveTextStyles').mockImplementation(async (styles) => styles)
    useKoharuStore.setState({
      textStyles: {
        default_style: { ...emptyTextStyle },
        presets: [
          { id: 'target', name: 'Target', key: null, style: { ...emptyTextStyle } },
          { id: 'other', name: 'Other preset', key: '1', style: { ...emptyTextStyle } },
        ],
      },
    })
    render(<PresetShortcutDialog id='target' onClose={vi.fn()} />)
    const recorder = screen.getByRole('button', { name: 'Record preset shortcut' })
    fireEvent.click(recorder)
    fireEvent.keyDown(recorder, event)
    expect(screen.getByRole('alert')).toHaveTextContent(owner)
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled()
    expect(save).not.toHaveBeenCalled()
  })

  it('records a preset combination and keeps the binding on that preset', async () => {
    const save = vi.spyOn(commands, 'saveTextStyles').mockImplementation(async (styles) => styles)
    useKoharuStore.setState({
      textStyles: {
        default_style: { ...emptyTextStyle },
        presets: [{ id: 'target', name: 'Target', key: null, style: { ...emptyTextStyle } }],
      },
    })
    render(<PresetShortcutDialog id='target' onClose={vi.fn()} />)
    const recorder = screen.getByRole('button', { name: 'Record preset shortcut' })
    fireEvent.click(recorder)
    fireEvent.keyDown(recorder, { key: '!', code: 'Digit1', ctrlKey: true, shiftKey: true })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        expect.objectContaining({
          presets: [expect.objectContaining({ id: 'target', key: 'ctrl+shift+1' })],
        }),
      ),
    )
    expect(commands.saveShortcuts).not.toHaveBeenCalled()
  })

  it('refuses a tool binding owned by a preset and shows its name', () => {
    useKoharuStore.setState({
      textStyles: {
        default_style: { ...emptyTextStyle },
        presets: [{ id: 'one', name: 'White outline', key: '1', style: { ...emptyTextStyle } }],
      },
    })
    open()
    record('Text', { key: '1' })
    expect(screen.getByText('Used by preset “White outline”')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Text · Primary shortcut' })).toHaveAttribute(
      'aria-invalid',
      'true',
    )
    expect(commands.saveShortcuts).not.toHaveBeenCalled()
    expect(useKoharuStore.getState().shortcuts.text).toBe('t')
  })

  it('saves the wheel choice and restores it with the complete shortcut defaults', async () => {
    const user = userEvent.setup()
    open()
    await user.click(screen.getByRole('combobox', { name: 'Zoom with' }))
    await user.click(await screen.findByRole('option', { name: 'Wheel alone' }))
    await waitFor(() => expect(commands.saveWheelZoom).toHaveBeenCalledWith('wheel'))
    expect(useKoharuStore.getState().wheelZoom).toBe('wheel')
    record('Zoom in', { key: 'z', altKey: true })
    fireEvent.click(screen.getByRole('button', { name: 'Reset to defaults' }))
    await waitFor(() => expect(commands.saveWheelZoom).toHaveBeenLastCalledWith('ctrl'))
    expect(useKoharuStore.getState().wheelZoom).toBe('ctrl')
    expect(useKoharuStore.getState().shortcuts).toEqual(defaultShortcuts)
  })

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
