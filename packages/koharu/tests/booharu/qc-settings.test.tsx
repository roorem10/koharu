import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { PipelinePreferences } from '@/components/preferences/PipelinePreferences'
import { minSizePercent, minSizeShare } from '@/lib/booharu/qc'
import { queryClient } from '@/lib/queries'

import { builtInLettering, builtInReading, builtInRuns, settingsDesktop } from './settings'

describe('Booharu quality check settings', () => {
  it('shows the smallest readable text as a percentage, and takes only 0.1% to 10%', () => {
    expect(minSizePercent(undefined)).toBe(1)
    expect(minSizePercent({})).toBe(1)
    expect(minSizePercent({ min_size: 0.007 })).toBe(0.7)
    expect(minSizePercent({ min_size: 0.0125 })).toBe(1.25)
    expect(minSizeShare(0.7)).toBe(0.007)
    expect(minSizeShare(1.25)).toBe(0.0125)
    expect(minSizeShare(0.1)).toBe(0.001)
    expect(minSizeShare(10)).toBe(0.1)
    for (const refused of [null, 0, 0.09, 10.01, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(minSizeShare(refused), String(refused)).toBeNull()
    }
    // Each step the field offers is saved and shown again as it was.
    for (let tenths = 1; tenths <= 100; tenths += 1) {
      expect(minSizePercent({ min_size: minSizeShare(tenths / 10)! })).toBe(tenths / 10)
    }
  })

  it('saves it to the quality check’s section once committed, over every section’s latest value', async () => {
    const { file, saves } = settingsDesktop()
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <PipelinePreferences value={{}} models={[]} onChange={() => undefined} />
      </QueryClientProvider>,
    )
    const heading = await screen.findByRole('heading', { name: 'Quality check', level: 3 })
    const section = within(heading.closest('section')!)
    const field = section.getByRole('textbox', { name: 'Smallest readable text' })
    await waitFor(() => expect(field).toBeEnabled())
    expect(field).toHaveValue('1')
    // Another writer changes another section after the page read the file.
    file.preferences['booharu-mcp'] = { port: 50000 }

    await user.clear(field)
    await user.type(field, '1.5')
    expect(saves).toHaveLength(0)
    await user.tab()
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]).toEqual({
      'booharu-mcp': { port: 50000 },
      'booharu-ocr-box': { default_mode: 'read' },
      'booharu-lettering': builtInLettering,
      'booharu-fonts': { favorites: [] },
      'booharu-runs': builtInRuns,
      'booharu-reading': builtInReading,
      'booharu-qc': { min_size: 0.015 },
      'booharu-start': { fresh_start_note_seen: false },
    })

    await user.click(section.getByRole('button', { name: 'Increase smallest readable text' }))
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(saves[1]!['booharu-qc']).toEqual({ min_size: 0.016 })
    await waitFor(() => expect(field).toHaveValue('1.6'))

    // A cleared field saves nothing and shows the saved value again.
    await user.clear(field)
    await user.tab()
    expect(field).toHaveValue('1.6')
    expect(saves).toHaveLength(2)
  })
})
