import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { PipelinePreferences } from '@/components/preferences/PipelinePreferences'
import { SettingsPage } from '@/components/preferences/SettingsPage'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Preferences } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { builtInRuns, settingsDesktop } from './settings'

const SWITCH = "Use the original lettering's colors"

/** Settings > Pipeline, where the switch sits in the Translator section. */
function renderPipeline() {
  render(
    <QueryClientProvider client={queryClient}>
      <PipelinePreferences value={{}} models={[]} onChange={() => undefined} />
    </QueryClientProvider>,
  )
}

/** The colour detection switch, once it shows the file's choice. */
async function detection(): Promise<HTMLElement> {
  const toggle = await screen.findByRole('switch', { name: SWITCH })
  await waitFor(() => expect(toggle).toBeEnabled())
  return toggle
}

describe('Booharu colour detection switch', () => {
  it("is on by default, and saves a choice over every section's latest value", async () => {
    const { file, saves } = settingsDesktop()
    const user = userEvent.setup()
    renderPipeline()

    const toggle = await detection()
    expect(toggle).toBeChecked()
    // Another writer changes another section, and the default text style, after the page read.
    file.preferences['booharu-mcp'] = { port: 50000 }
    file.preferences['booharu-lettering'].default = {
      fill: [1, 0, 0, 1],
      stroke: null,
      stroke_width: 1.5,
    }

    await user.click(toggle)
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]).toEqual({
      'booharu-mcp': { port: 50000 },
      'booharu-ocr-box': { default_mode: 'read' },
      'booharu-lettering': {
        default: { fill: [1, 0, 0, 1], stroke: null, stroke_width: 1.5 },
        detect_colors: false,
      },
      'booharu-fonts': { favorites: [] },
      'booharu-runs': builtInRuns,
    })
    await waitFor(() => expect(toggle).not.toBeChecked())
  })

  it("shows a failed save's error, then the file's choice", async () => {
    const { save } = settingsDesktop({
      default: { fill: null, stroke: null, stroke_width: 1.5 },
      detect_colors: false,
    })
    let fail: (reason: unknown) => void = () => undefined
    save.mockImplementation(() => new Promise((_, reject) => (fail = reject)))
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    renderPipeline()

    const toggle = await detection()
    expect(toggle).not.toBeChecked()
    await user.click(toggle)
    await waitFor(() => expect(save).toHaveBeenCalled())
    // The choice being saved shows until the save ends.
    expect(toggle).toBeChecked()

    fail('config.toml is read-only')
    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'error', description: 'config.toml is read-only' }),
      ),
    )
    await waitFor(() => expect(toggle).not.toBeChecked())
  })

  it('links its note to the default text style on the Typesetting tab', async () => {
    settingsDesktop()
    const preferences: Preferences = {
      pipeline: {},
      renderer: { font_families: ['CCWildWords'] },
      agent: {},
      providers: {},
    }
    vi.spyOn(commands, 'getPreferences').mockResolvedValue(preferences)
    vi.spyOn(commands, 'listModels').mockResolvedValue([])
    useKoharuStore.setState({ settingsOpen: true, preferences })
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <SettingsPage />
      </QueryClientProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Pipeline' }))
    await user.click(await screen.findByRole('button', { name: 'default text colors' }))

    const section = await screen.findByRole('region', { name: 'Default text style' })
    expect(screen.getByRole('heading', { name: 'Renderer', level: 2 })).toBeInTheDocument()
    await waitFor(() => expect(section).toHaveFocus())
    expect(vi.mocked(Element.prototype.scrollIntoView).mock.contexts).toContain(section)
  })
})
