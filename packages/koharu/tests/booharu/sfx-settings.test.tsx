import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { PipelinePreferences } from '@/components/preferences/PipelinePreferences'
import { queryClient } from '@/lib/queries'
import { toast } from '@koharu/ui/components/toast'

import { builtInLettering, builtInReading, builtInRuns, settingsDesktop } from './settings'

const NOTE =
  'Koharu marks every sound effect it finds. Pick another category for a box to translate it anyway, or add a subtitle from its menu.'

/** Settings > Pipeline, where the sound effects row sits in the Translator section. */
function renderPipeline() {
  render(
    <QueryClientProvider client={queryClient}>
      <PipelinePreferences value={{}} models={[]} onChange={() => undefined} />
    </QueryClientProvider>,
  )
}

/** The mode `select` shows. */
function shownMode(select: HTMLElement): string | null | undefined {
  return select.querySelector('[data-slot="select-value"]')?.textContent
}

/** The sound effects mode, once the file's value shows. */
async function modeSelect(shown: string): Promise<HTMLElement> {
  const select = await screen.findByRole('combobox', { name: 'Sound effects' })
  await waitFor(() => expect(shownMode(select)).toBe(shown))
  return select
}

describe('Booharu sound effects setting', () => {
  it("keeps sound effects as art by default, and saves a choice over every section's latest value", async () => {
    const { file, saves } = settingsDesktop()
    const user = userEvent.setup()
    renderPipeline()

    const select = await modeSelect('Keep as art')
    // What the chosen mode does, and how a box gets out of it, read with the choice.
    expect(select).toHaveAccessibleDescription(
      `Leave sound effects as the original art: not translated, not erased. ${NOTE}`,
    )
    // Another writer changes another section, and the lettering section's switch, after the page
    // read the file.
    file.preferences['booharu-mcp'] = { port: 50000 }
    file.preferences['booharu-lettering'].auto = true

    await user.click(select)
    // Each mode in the list says what it does.
    expect(screen.getByRole('option', { name: /^Inside bubbles only/ })).toHaveTextContent(
      'Translate sound effects inside speech bubbles; keep the rest as art.',
    )
    await user.click(screen.getByRole('option', { name: /^Translate all/ }))

    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]).toEqual({
      'booharu-mcp': { port: 50000 },
      'booharu-ocr-box': { default_mode: 'read' },
      'booharu-lettering': { ...builtInLettering, auto: true, sfx: 'translate_all' },
      'booharu-fonts': { favorites: [] },
      'booharu-runs': builtInRuns,
      'booharu-reading': builtInReading,
      'booharu-qc': { min_size: 0.01 },
      'booharu-start': { fresh_start_note_seen: false },
    })
    await waitFor(() => expect(shownMode(select)).toBe('Translate all'))
    expect(select).toHaveAccessibleDescription(`Translate and letter every sound effect. ${NOTE}`)
  })

  it("shows a choice while it saves, and a failed save's error and the file's mode after", async () => {
    const { save } = settingsDesktop({ ...builtInLettering, sfx: 'inside_bubbles' })
    let fail: (reason: unknown) => void = () => undefined
    save.mockImplementation(() => new Promise((_, reject) => (fail = reject)))
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    renderPipeline()

    const select = await modeSelect('Inside bubbles only')
    await user.click(select)
    await user.click(await screen.findByRole('option', { name: /^Keep as art/ }))
    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(save.mock.calls[0]![0]['booharu-lettering'].sfx).toBe('skip')
    expect(shownMode(select)).toBe('Keep as art')

    fail('config.toml is read-only')
    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'error', description: 'config.toml is read-only' }),
      ),
    )
    await waitFor(() => expect(shownMode(select)).toBe('Inside bubbles only'))
  })
})
