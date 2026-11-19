import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { SettingsPage } from '@/components/preferences/SettingsPage'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Preferences } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { builtInLettering, builtInReading, builtInRuns, settingsDesktop } from './settings'

const preferences: Preferences = {
  pipeline: {},
  renderer: { font_families: [] },
  agent: {},
  providers: {},
}

/** Settings, opened on its Runs tab, once the run settings show. */
async function openRuns(user: ReturnType<typeof userEvent.setup>) {
  vi.spyOn(commands, 'getPreferences').mockResolvedValue(preferences)
  vi.spyOn(commands, 'listModels').mockResolvedValue([])
  useKoharuStore.setState({ settingsOpen: true, preferences })
  render(
    <QueryClientProvider client={queryClient}>
      <SettingsPage />
    </QueryClientProvider>,
  )
  await user.click(screen.getByRole('button', { name: 'Runs' }))
  expect(screen.getByRole('heading', { name: 'Runs', level: 2 })).toBeInTheDocument()
  await screen.findByRole('group', { name: 'Detect + OCR' })
}

/** A preset's row, by the name it shows. */
const preset = (name: string) => within(screen.getByRole('group', { name }))

/** The names of the presets a save sent, with their stages. */
const presetsOf = (save: { 'booharu-runs': { presets?: unknown } }) => save['booharu-runs'].presets

describe('Booharu Runs settings', () => {
  it("saves a change to the run settings alone, over every section's latest value", async () => {
    const { file, saves } = settingsDesktop()
    const user = userEvent.setup()
    await openRuns(user)

    expect(screen.getByRole('textbox', { name: 'Retries per failed stage' })).toHaveValue('2')
    expect(screen.getByRole('radio', { name: 'Keep going with the other pages' })).toBeChecked()
    expect(screen.getByRole('radio', { name: 'Page by page' })).toBeChecked()
    expect(screen.getByRole('radio', { name: 'One stage at a time' })).toHaveAccessibleDescription(
      'Loads one model at a time, which uses less graphics memory.',
    )
    expect(screen.getByRole('switch', { name: 'Play a finish sound' })).toBeChecked()

    // Another writer changes other sections, and a preset, after the page read the file.
    file.preferences['booharu-mcp'] = { port: 50000 }
    file.preferences['booharu-lettering'].detect_colors = false
    file.preferences['booharu-runs'].presets = [
      ...builtInRuns.presets,
      { name: 'Clean only', stages: ['inpaint'] },
    ]

    await user.click(screen.getByRole('radio', { name: 'Stop the run' }))
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]).toEqual({
      'booharu-mcp': { port: 50000 },
      'booharu-ocr-box': { default_mode: 'read' },
      'booharu-lettering': { ...builtInLettering, detect_colors: false },
      'booharu-fonts': { favorites: [] },
      'booharu-runs': {
        ...builtInRuns,
        presets: [...builtInRuns.presets, { name: 'Clean only', stages: ['inpaint'] }],
        on_page_failure: 'stop',
      },
      'booharu-reading': builtInReading,
      'booharu-start': { fresh_start_note_seen: false },
    })
    // The file's state shows once the save ends, the other writer's preset with it.
    expect(await screen.findByRole('group', { name: 'Clean only' })).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'Stop the run' })).toBeChecked()

    await user.click(screen.getByRole('radio', { name: 'One stage at a time' }))
    await user.click(screen.getByRole('switch', { name: 'Play a finish sound' }))
    await waitFor(() => expect(saves).toHaveLength(3))
    expect(file.preferences['booharu-runs']).toMatchObject({
      on_page_failure: 'stop',
      order: 'stages',
      finish_sound: false,
    })
    expect(file.preferences['booharu-mcp']).toEqual({ port: 50000 })
  })

  it('adds a preset, ticks its stages, renames it and deletes another', async () => {
    const { file, saves } = settingsDesktop()
    const user = userEvent.setup()
    await openRuns(user)

    await user.type(screen.getByRole('textbox', { name: 'New preset name' }), ' Clean only {Enter}')
    await waitFor(() => expect(saves).toHaveLength(1))
    // A new preset has every stage.
    expect(presetsOf(saves[0]!)).toEqual([
      ...builtInRuns.presets,
      { name: 'Clean only', stages: ['detect', 'recognize', 'inpaint', 'translate'] },
    ])
    expect(screen.getByRole('textbox', { name: 'New preset name' })).toHaveValue('')

    const clean = preset('Clean only')
    await user.click(clean.getByRole('checkbox', { name: 'Detection' }))
    await user.click(clean.getByRole('checkbox', { name: 'OCR' }))
    await user.click(clean.getByRole('checkbox', { name: 'Translation' }))
    await waitFor(() => expect(saves).toHaveLength(4))
    expect(file.preferences['booharu-runs'].presets?.[2]).toEqual({
      name: 'Clean only',
      stages: ['inpaint'],
    })
    // Its last stage can't be switched off.
    expect(clean.getByRole('checkbox', { name: 'Inpainting' })).toBeChecked()
    expect(clean.getByRole('checkbox', { name: 'Inpainting' })).toHaveAttribute(
      'aria-disabled',
      'true',
    )

    const name = clean.getByRole('textbox', { name: 'Preset name' })
    await user.clear(name)
    await user.type(name, 'Inpaint only{Enter}')
    await waitFor(() => expect(saves).toHaveLength(5))
    expect(presetsOf(saves[4]!)).toEqual([
      ...builtInRuns.presets,
      { name: 'Inpaint only', stages: ['inpaint'] },
    ])
    expect(await screen.findByRole('group', { name: 'Inpaint only' })).toBeInTheDocument()

    await user.click(preset('Detect + OCR').getByRole('button', { name: 'Delete' }))
    await waitFor(() => expect(saves).toHaveLength(6))
    expect(file.preferences['booharu-runs'].presets).toEqual([
      builtInRuns.presets[1],
      { name: 'Inpaint only', stages: ['inpaint'] },
    ])
    await waitFor(() => expect(screen.queryByRole('group', { name: 'Detect + OCR' })).toBeNull())
  })

  it('refuses a blank name, or one another preset has, with a message', async () => {
    const { saves } = settingsDesktop()
    const user = userEvent.setup()
    await openRuns(user)

    const name = preset('Detect + OCR').getByRole('textbox', { name: 'Preset name' })
    await user.clear(name)
    await user.keyboard('{Enter}')
    expect(name).toHaveAccessibleDescription('Give the preset a name.')
    expect(name).toBeInvalid()

    await user.type(name, ' Translate + clean up{Enter}')
    expect(name).toHaveAccessibleDescription(
      'Another preset is already named “Translate + clean up”.',
    )
    // Escape puts the saved name back.
    name.focus()
    await user.keyboard('{Escape}')
    expect(name).toHaveValue('Detect + OCR')
    expect(name).toBeValid()

    const added = screen.getByRole('textbox', { name: 'New preset name' })
    await user.click(screen.getByRole('button', { name: 'Add' }))
    expect(added).toHaveAccessibleDescription('Give the preset a name.')
    await user.type(added, 'Detect + OCR{Enter}')
    expect(added).toHaveAccessibleDescription('Another preset is already named “Detect + OCR”.')
    await user.type(added, '!')
    expect(added).toBeValid()

    expect(saves).toEqual([])
  })

  it('keeps the retries within 0 to 5', async () => {
    const { saves } = settingsDesktop()
    const user = userEvent.setup()
    await openRuns(user)

    const retries = screen.getByRole('textbox', { name: 'Retries per failed stage' })
    await user.clear(retries)
    await user.type(retries, '9')
    await user.tab()
    await waitFor(() => expect(saves.at(-1)?.['booharu-runs'].retries).toBe(5))
    expect(retries).toHaveValue('5')
    expect(screen.getByRole('button', { name: 'More retries' })).toBeDisabled()

    await user.clear(retries)
    await user.type(retries, '0')
    await user.tab()
    await waitFor(() => expect(saves.at(-1)?.['booharu-runs'].retries).toBe(0))
    expect(screen.getByRole('button', { name: 'Fewer retries' })).toBeDisabled()
    expect(saves.every((save) => (save['booharu-runs'].retries ?? 0) <= 5)).toBe(true)
  })

  it("shows a failed save's error, then the file's settings", async () => {
    const { save } = settingsDesktop()
    save.mockRejectedValue('config.toml is read-only')
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    await openRuns(user)

    await user.click(preset('Translate + clean up').getByRole('button', { name: 'Delete' }))
    // As every failed command shows its error.
    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'error', description: 'config.toml is read-only' }),
      ),
    )
    expect(await screen.findByRole('group', { name: 'Translate + clean up' })).toBeInTheDocument()
  })

  it("shows the run settings without waiting for Koharu's own settings", async () => {
    settingsDesktop()
    vi.spyOn(commands, 'getPreferences').mockRejectedValue('config.toml is unreadable')
    vi.spyOn(commands, 'listModels').mockResolvedValue([])
    useKoharuStore.setState({ settingsOpen: true, preferences: null })
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <SettingsPage />
      </QueryClientProvider>,
    )
    // Koharu's tabs wait for its settings.
    await user.click(screen.getByRole('button', { name: 'Pipeline' }))
    expect(screen.getByText('Loading preferences…')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Runs' }))
    await screen.findByRole('group', { name: 'Detect + OCR' })
    expect(screen.queryByText('Loading preferences…')).toBeNull()
  })
})
