import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { TypesettingPreferences } from '@/components/preferences/TypesettingPreferences'
import { SUBTITLE_PERCENT, subtitlePercent, subtitleShare } from '@/lib/booharu/sfx'
import { queryClient } from '@/lib/queries'
import { commands, type FontFamily } from '@koharu/ipc/protocol'

import { fontFamilies } from './fonts'
import { builtInLettering, builtInReading, builtInRuns, settingsDesktop } from './settings'

const SIZE = 'Sound effect subtitle size'

/** Settings > Typesetting, where the Sound effect subtitles card ends the page, and `fonts` the
 * families Koharu lists, each preview a blank image. */
function renderTypesetting(fonts: FontFamily[] = []) {
  vi.spyOn(commands, 'listFonts').mockResolvedValue(fonts)
  vi.spyOn(commands, 'getFontPreview').mockResolvedValue(new ArrayBuffer(0))
  render(
    <QueryClientProvider client={queryClient}>
      <TypesettingPreferences
        value={{ font_families: ['CCWildWords'] }}
        onChange={() => undefined}
      />
    </QueryClientProvider>,
  )
}

/** The Sound effect subtitles card, once it shows the settings file. */
async function card() {
  const heading = await screen.findByRole('heading', { name: 'Sound effect subtitles', level: 3 })
  const region = within(heading.closest('section')!)
  await waitFor(() => expect(region.getByRole('textbox', { name: SIZE })).toBeEnabled())
  return region
}

/** The subtitle look each save sent. */
function savedLooks(saves: ReturnType<typeof settingsDesktop>['saves']) {
  return saves.map((preferences) => preferences['booharu-lettering'].subtitle)
}

describe('Booharu sound effect subtitles settings', () => {
  it("shows the size as a percentage of the page's shorter side, and takes only 0.1% to 10%", () => {
    expect(subtitlePercent(0.015)).toBe(1.5)
    expect(subtitlePercent(0.0125)).toBe(1.25)
    expect(subtitlePercent(0.001)).toBe(0.1)
    expect(subtitlePercent(0.1)).toBe(10)
    expect(subtitleShare(1.5)).toBe(0.015)
    expect(subtitleShare(1.25)).toBe(0.0125)
    expect(subtitleShare(SUBTITLE_PERCENT.min)).toBe(0.001)
    expect(subtitleShare(SUBTITLE_PERCENT.max)).toBe(0.1)
    for (const refused of [null, 0, -1.5, 0.09, 10.01, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(subtitleShare(refused), String(refused)).toBeNull()
    }
    // Each step the field offers is saved and shown again as it was.
    for (let tenths = 1; tenths <= 100; tenths += 1) {
      expect(subtitlePercent(subtitleShare(tenths / 10)!)).toBe(tenths / 10)
    }
  })

  it("saves a committed size over every section's latest value, and keeps it within 10%", async () => {
    const { file, saves } = settingsDesktop()
    const user = userEvent.setup()
    renderTypesetting()
    const region = await card()
    const size = region.getByRole('textbox', { name: SIZE })
    expect(size).toHaveValue('1.5')
    // Another writer changes another section, and the lettering section's switch, after the page
    // read the file.
    file.preferences['booharu-mcp'] = { port: 50000 }
    file.preferences['booharu-lettering'].auto = true

    await user.clear(size)
    await user.type(size, '2')
    expect(saves).toHaveLength(0)
    await user.tab()
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]).toEqual({
      'booharu-mcp': { port: 50000 },
      'booharu-ocr-box': { default_mode: 'read' },
      'booharu-lettering': {
        ...builtInLettering,
        auto: true,
        subtitle: { ...builtInLettering.subtitle, size: 0.02 },
      },
      'booharu-fonts': { favorites: [] },
      'booharu-runs': builtInRuns,
      'booharu-reading': builtInReading,
      'booharu-qc': { min_size: 0.01 },
      'booharu-start': { fresh_start_note_seen: false },
    })

    await user.click(region.getByRole('button', { name: 'Increase sound effect subtitle size' }))
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(savedLooks(saves)[1]).toEqual({ ...builtInLettering.subtitle, size: 0.021 })
    await waitFor(() => expect(size).toHaveValue('2.1'))

    // A cleared field saves nothing and shows the saved size again.
    await user.clear(size)
    await user.tab()
    expect(size).toHaveValue('2.1')
    expect(saves).toHaveLength(2)

    // A size past 10% is kept at 10%.
    await user.clear(size)
    await user.type(size, '25')
    await user.tab()
    await waitFor(() => expect(saves).toHaveLength(3))
    expect(savedLooks(saves)[2]).toEqual({ ...builtInLettering.subtitle, size: 0.1 })
    await waitFor(() => expect(size).toHaveValue('10'))
  })

  it('saves a font, the default font again, and a colour, each at once', async () => {
    const { saves } = settingsDesktop()
    const user = userEvent.setup()
    renderTypesetting(fontFamilies('Anime Ace', 'CCWildWords'))
    const region = await card()

    const font = region.getByRole('button', { name: 'Sound effect subtitle font' })
    expect(font).toHaveTextContent('Default font')
    await user.click(font)
    const list = await screen.findByRole('listbox', { name: 'Fonts' })
    expect(
      within(list)
        .getAllByRole('option')
        .map((option) => option.getAttribute('aria-label')),
    ).toEqual(['Default font', 'Anime Ace', 'CCWildWords'])
    await user.click(within(list).getByRole('option', { name: 'Anime Ace' }))
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(savedLooks(saves)[0]).toEqual({ ...builtInLettering.subtitle, family: 'Anime Ace' })

    // The leading row saves no family: the default font.
    await user.click(font)
    await user.click(await screen.findByRole('option', { name: 'Default font' }))
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(savedLooks(saves)[1]).toEqual(builtInLettering.subtitle)

    await user.click(region.getByRole('button', { name: 'Sound effect subtitle outline color' }))
    const hex = await screen.findByRole('textbox', { name: 'Hex color code' })
    await user.clear(hex)
    // Whole, as typing it would pick each three-digit colour on the way.
    await user.paste('FFFF00')
    await user.keyboard('{Escape}')
    await waitFor(() => expect(saves).toHaveLength(3))
    expect(savedLooks(saves)[2]).toEqual({ ...builtInLettering.subtitle, stroke: [1, 1, 0, 1] })
    expect(region.getByText('#FFFF00')).toBeInTheDocument()
  })
})
