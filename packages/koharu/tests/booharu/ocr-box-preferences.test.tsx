import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { PipelinePreferences } from '@/components/preferences/PipelinePreferences'
import { queryClient } from '@/lib/queries'
import { commands, type BooharuPreferences } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { builtInEmphasis, builtInReading, builtInRuns } from './settings'

/** The sections these tests leave alone, as the desktop answers them. */
const OTHERS = {
  'booharu-lettering': {
    default: { fill: null, stroke: null, stroke_width: 1.5 },
    detect_colors: true,
    auto: false,
    categories: {},
    small_text: 0.012,
    model_confidence: 0.6,
    sfx: 'skip',
    subtitle: { family: null, size: 0.015, fill: [0, 0, 0, 1], stroke: [1, 1, 1, 1] },
  },
  'booharu-fonts': { favorites: [] },
  'booharu-runs': builtInRuns,
  'booharu-reading': builtInReading,
  'booharu-qc': { min_size: 0.01 },
  'booharu-start': { fresh_start_note_seen: false },
  'booharu-emphasis': builtInEmphasis,
} satisfies Partial<BooharuPreferences>

/** The Booharu settings the desktop's file holds, which another writer, such as the agent's
 * settings tool, may change while the page shows them; `saves` lists what each save sent. */
function desktop() {
  const file: { preferences: BooharuPreferences } = {
    preferences: {
      'booharu-mcp': { port: 47821 },
      'booharu-ocr-box': { default_mode: 'read' },
      ...OTHERS,
    },
  }
  const saves: BooharuPreferences[] = []
  vi.spyOn(commands, 'booharuGetPreferences').mockImplementation(async () =>
    structuredClone(file.preferences),
  )
  const save = vi
    .spyOn(commands, 'booharuSavePreferences')
    .mockImplementation(async (preferences) => {
      saves.push(structuredClone(preferences))
      file.preferences = structuredClone(preferences)
      return structuredClone(file.preferences)
    })
  return { file, saves, save }
}

/** Settings > Pipeline, where the O tool's section sits after the stages. */
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

/** The O tool's default mode, once the file's value shows. */
async function modeSelect(shown: string): Promise<HTMLElement> {
  const select = await screen.findByRole('combobox', { name: 'Default action' })
  await waitFor(() => expect(shownMode(select)).toBe(shown))
  return select
}

describe('Booharu O tool settings', () => {
  it("reads the mode and saves a choice over every section's latest value", async () => {
    const { file, saves } = desktop()
    const user = userEvent.setup()
    renderPipeline()

    expect(screen.getByRole('heading', { name: 'OCR box' })).toBeInTheDocument()
    const select = await modeSelect('Read only')
    // Another writer changes another section after the page read the file.
    file.preferences['booharu-mcp'] = { port: 50000 }

    await user.click(select)
    await user.click(await screen.findByRole('option', { name: 'Read, translate and clean' }))

    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]).toEqual({
      'booharu-mcp': { port: 50000 },
      'booharu-ocr-box': { default_mode: 'read_translate_clean' },
      ...OTHERS,
    })
    await waitFor(() => expect(shownMode(select)).toBe('Read, translate and clean'))
  })

  it("shows a choice while it saves, and a failed save's error and the file's mode after", async () => {
    const { save } = desktop()
    let fail: (reason: unknown) => void = () => undefined
    save.mockImplementation(() => new Promise((_, reject) => (fail = reject)))
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    renderPipeline()

    const select = await modeSelect('Read only')
    await user.click(select)
    await user.click(await screen.findByRole('option', { name: 'Read and translate' }))
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith({
        'booharu-mcp': { port: 47821 },
        'booharu-ocr-box': { default_mode: 'read_translate' },
        ...OTHERS,
      }),
    )
    expect(shownMode(select)).toBe('Read and translate')

    fail('config.toml is read-only')
    // As every failed command shows its error.
    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'error', description: 'config.toml is read-only' }),
      ),
    )
    await waitFor(() => expect(shownMode(select)).toBe('Read only'))
    // Once: the save's mutation shows it, and nothing shows it again.
    const failures = shown.mock.calls.filter(
      ([shownToast]) => shownToast.description === 'config.toml is read-only',
    )
    expect(failures).toHaveLength(1)
  })
})
