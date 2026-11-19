import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { PipelinePreferences } from '@/components/preferences/PipelinePreferences'
import { queryClient } from '@/lib/queries'

import { gemma, indexTranslate, models, readingDesktop, textOnly } from './reading'
import { builtInLettering, builtInReading, builtInRuns } from './settings'

/** Settings > Pipeline, where Read chapter's section follows the translator's. */
function renderPipeline() {
  render(
    <QueryClientProvider client={queryClient}>
      <PipelinePreferences value={{}} models={models} onChange={() => undefined} />
    </QueryClientProvider>,
  )
}

/** Read chapter's section, once it shows the file's values. */
async function readingSection() {
  const section = within(
    (await screen.findByRole('heading', { name: 'Read chapter', level: 3 })).closest('section')!,
  )
  await waitFor(() =>
    expect(section.getByRole('switch', { name: 'Ask before reading a chapter' })).toBeEnabled(),
  )
  return section
}

describe('Booharu Read chapter settings', () => {
  it('saves each row to the reading section at once, over every section’s latest value', async () => {
    const { file, saves } = readingDesktop({ translation: indexTranslate.reference })
    const user = userEvent.setup()
    renderPipeline()
    const section = await readingSection()

    const model = section.getByRole('button', { name: 'Read chapter model' })
    expect(model).toHaveTextContent('Same as translation model')
    // The translation model only translates, so a read with it refuses.
    expect(section.getByRole('status')).toHaveTextContent(
      "Index-Translate 9B only translates, so it can't read the chapter.",
    )
    // Another writer changes another section after the page read the file.
    file.preferences['booharu-mcp'] = { port: 50000 }

    await user.click(model)
    const picker = within(await screen.findByRole('listbox', { name: 'Choose a model' }))
    expect(picker.getAllByRole('option').map((option) => option.textContent)).toEqual([
      'GPT-5.4',
      'Gemma 4 12B',
    ])
    await user.click(picker.getByRole('option', { name: 'Use Gemma 4 12B from Koharu' }))
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]).toEqual({
      'booharu-mcp': { port: 50000 },
      'booharu-ocr-box': { default_mode: 'read' },
      'booharu-lettering': builtInLettering,
      'booharu-fonts': { favorites: [] },
      'booharu-runs': builtInRuns,
      'booharu-reading': { ...builtInReading, model: gemma.reference },
      'booharu-start': { fresh_start_note_seen: false },
    })
    await waitFor(() => expect(model).toHaveTextContent('Gemma 4 12B'))
    expect(section.queryByRole('status')).toBeNull()

    await user.click(section.getByRole('switch', { name: 'Ask before reading a chapter' }))
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(saves[1]!['booharu-reading']).toEqual({
      model: gemma.reference,
      ask: false,
      pages_per_request: 20,
    })

    await user.click(section.getByRole('button', { name: 'More pages' }))
    await waitFor(() => expect(saves).toHaveLength(3))
    expect(saves[2]!['booharu-reading']?.pages_per_request).toBe(21)
    const pages = section.getByRole('textbox', { name: 'Pages per Read chapter request' })
    await user.clear(pages)
    await user.type(pages, '8')
    await user.tab()
    await waitFor(() => expect(saves).toHaveLength(4))
    expect(saves[3]!['booharu-reading']?.pages_per_request).toBe(8)

    await user.click(model)
    await user.click(await screen.findByRole('button', { name: 'Same as translation model' }))
    await waitFor(() => expect(saves).toHaveLength(5))
    expect(saves[4]!['booharu-reading']).toEqual({ model: null, ask: false, pages_per_request: 8 })
  })

  it('says why a saved model that can’t see images can’t read', async () => {
    readingDesktop({ saved: textOnly.reference })
    renderPipeline()
    const section = await readingSection()
    expect(section.getByRole('button', { name: 'Read chapter model' })).toHaveTextContent(
      'Text Only',
    )
    expect(section.getByRole('status')).toHaveTextContent(
      "Text Only has no vision capabilities, so it can't see the pages.",
    )
  })
})
