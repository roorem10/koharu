import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { TypesettingPreferences } from '@/components/preferences/TypesettingPreferences'
import {
  EMPHASIS_PERCENT,
  emphasisFactor,
  emphasisPercent,
  emphasisWeight,
  emphasisWidth,
} from '@/lib/booharu/emphasis'
import { queryClient } from '@/lib/queries'
import {
  commands,
  type EmphasisSettings,
  type EmphasisStyle,
  type FontFamily,
} from '@koharu/ipc/protocol'

import { fontFamilies } from './fonts'
import {
  builtInEmphasis,
  builtInLettering,
  builtInReading,
  builtInRuns,
  settingsDesktop,
} from './settings'

const WEIGHT = 'Emphasis weight'
const SIZE = 'Emphasis size (%)'

/** An emphasis style setting nothing but `values`, as the desktop answers it. */
function style(values: EmphasisStyle) {
  return {
    family: null,
    weight: null,
    italic: null,
    size_factor: null,
    fill: null,
    stroke: null,
    stroke_width: null,
    outer_stroke: null,
    outer_stroke_width: null,
    ...values,
  }
}

/** Settings > Typesetting, where the Emphasis card ends the page, and `fonts` the families Koharu
 * lists, each preview a blank image. */
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

/** The Emphasis card, once it shows the settings file. */
async function card() {
  const heading = await screen.findByRole('heading', { name: 'Emphasis', level: 3 })
  const region = within(heading.closest('section')!)
  await waitFor(() => expect(region.getByRole('textbox', { name: WEIGHT })).toBeEnabled())
  return region
}

/** The emphasis section each save sent. */
function savedEmphasis(saves: ReturnType<typeof settingsDesktop>['saves']): EmphasisSettings[] {
  return saves.map((preferences) => preferences['booharu-emphasis'])
}

/** What the select `trigger` shows. */
function shown(trigger: HTMLElement): string | null | undefined {
  return trigger.querySelector('[data-slot="select-value"]')?.textContent
}

/** Picks `option` in the select named `name` in `region`. */
async function choose(
  user: ReturnType<typeof userEvent.setup>,
  region: ReturnType<typeof within>,
  name: string,
  option: string,
) {
  await user.click(region.getByRole('combobox', { name }))
  await user.click(await screen.findByRole('option', { name: option }))
}

/** Types `value` into the number field named `name` in `region` and commits it. */
async function typeNumber(
  user: ReturnType<typeof userEvent.setup>,
  region: ReturnType<typeof within>,
  name: string,
  value: string,
) {
  const field = region.getByRole('textbox', { name })
  await user.clear(field)
  if (value) await user.type(field, value)
  await user.tab()
}

/** Picks `hex` in the colour well named `name` in `region`. */
async function pickColour(
  user: ReturnType<typeof userEvent.setup>,
  region: ReturnType<typeof within>,
  name: string,
  hex: string,
) {
  await user.click(region.getByRole('button', { name }))
  const code = await screen.findByRole('textbox', { name: 'Hex color code' })
  await user.clear(code)
  // Whole, as typing it would pick each three-digit colour on the way.
  await user.paste(hex)
  await user.keyboard('{Escape}')
}

describe('Booharu emphasis settings', () => {
  it("shows a size factor as a percentage of the text's size, and takes only 25% to 400%", () => {
    expect(emphasisPercent(1.15)).toBe(115)
    expect(emphasisPercent(0.25)).toBe(25)
    expect(emphasisPercent(4)).toBe(400)
    expect(emphasisPercent(1.3333)).toBe(133.33)
    expect(emphasisFactor(115)).toBe(1.15)
    expect(emphasisFactor(133.33)).toBe(1.3333)
    expect(emphasisFactor(EMPHASIS_PERCENT.min)).toBe(0.25)
    expect(emphasisFactor(EMPHASIS_PERCENT.max)).toBe(4)
    for (const refused of [0, 24.99, 400.01, -115, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(emphasisFactor(refused), String(refused)).toBeNull()
    }
    // Each step the field offers is saved and shown again as it was.
    for (let percent = EMPHASIS_PERCENT.min; percent <= EMPHASIS_PERCENT.max; percent += 1) {
      expect(emphasisPercent(emphasisFactor(percent)!)).toBe(percent)
    }

    // Weights are whole, from 1 to 1000; outlines from 0 to 100 pixels wide.
    expect(emphasisWeight(650.4)).toBe(650)
    expect(emphasisWeight(1)).toBe(1)
    expect(emphasisWeight(1000)).toBe(1000)
    for (const refused of [0, 1001, -700, Number.NaN]) {
      expect(emphasisWeight(refused), String(refused)).toBeNull()
    }
    expect(emphasisWidth(0)).toBe(0)
    expect(emphasisWidth(2.5)).toBe(2.5)
    expect(emphasisWidth(100)).toBe(100)
    for (const refused of [-0.5, 100.5, Number.NaN]) {
      expect(emphasisWidth(refused), String(refused)).toBeNull()
    }
  })

  it("saves the AI box, the font, weight and italic of the default over every section's latest value", async () => {
    const { file, saves } = settingsDesktop()
    const user = userEvent.setup()
    renderTypesetting(fontFamilies('Anime Ace', 'CCWildWords'))
    const region = await card()

    const ai = region.getByRole('checkbox', { name: 'Let the AI emphasize words' })
    expect(ai).toBeChecked()
    expect(ai).toHaveAccessibleDescription(
      "Used with general chat models. Koharu's own translation models translate without it.",
    )
    expect(shown(region.getByRole('combobox', { name: 'Emphasis category' }))).toBe(
      'Default emphasis',
    )
    expect(region.getByRole('textbox', { name: WEIGHT })).toHaveValue('700')
    expect(region.getByRole('textbox', { name: SIZE })).toHaveValue('115')
    // What the default leaves unset uses the box's style.
    expect(region.getByRole('button', { name: 'Emphasis font' })).toHaveTextContent('Use box style')
    expect(shown(region.getByRole('combobox', { name: 'Emphasis italic' }))).toBe('Use box style')
    expect(region.getByRole('textbox', { name: 'Emphasis outline width' })).toHaveAttribute(
      'placeholder',
      'Use box style',
    )

    // Another writer changes another section, and the emphasis's size, after the page read the
    // file.
    file.preferences['booharu-mcp'] = { port: 50000 }
    file.preferences['booharu-emphasis'].default!.size_factor = 1.5

    await user.click(ai)
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]).toEqual({
      'booharu-mcp': { port: 50000 },
      'booharu-ocr-box': { default_mode: 'read' },
      'booharu-lettering': builtInLettering,
      'booharu-fonts': { favorites: [] },
      'booharu-runs': builtInRuns,
      'booharu-reading': builtInReading,
      'booharu-qc': { min_size: 0.01 },
      'booharu-start': { fresh_start_note_seen: false },
      'booharu-emphasis': {
        ...builtInEmphasis,
        ai: false,
        default: { ...builtInEmphasis.default, size_factor: 1.5 },
      },
    })
    await waitFor(() => expect(ai).not.toBeChecked())
    expect(region.getByRole('textbox', { name: SIZE })).toHaveValue('150')

    await user.click(region.getByRole('button', { name: 'Emphasis font' }))
    const list = await screen.findByRole('listbox', { name: 'Fonts' })
    expect(
      within(list)
        .getAllByRole('option')
        .map((option) => option.getAttribute('aria-label')),
    ).toEqual(['Use box style', 'Anime Ace', 'CCWildWords'])
    await user.click(within(list).getByRole('option', { name: 'Anime Ace' }))
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(savedEmphasis(saves)[1].default).toEqual(
      style({ family: 'Anime Ace', weight: 700, size_factor: 1.5 }),
    )

    await typeNumber(user, region, WEIGHT, '900')
    await waitFor(() => expect(saves).toHaveLength(3))
    expect(savedEmphasis(saves)[2].default?.weight).toBe(900)

    await choose(user, region, 'Emphasis italic', 'Italic')
    await waitFor(() => expect(saves).toHaveLength(4))
    expect(savedEmphasis(saves)[3].default?.italic).toBe(true)
    await choose(user, region, 'Emphasis italic', 'Normal')
    await waitFor(() => expect(saves).toHaveLength(5))
    expect(savedEmphasis(saves)[4].default).toEqual(
      style({ family: 'Anime Ace', weight: 900, italic: false, size_factor: 1.5 }),
    )
    expect(savedEmphasis(saves)[4].ai).toBe(false)
  })

  it('saves the size as a factor, kept within 25% to 400%, and whole weights within 1 to 1000', async () => {
    const { saves } = settingsDesktop()
    const user = userEvent.setup()
    renderTypesetting()
    const region = await card()
    const size = region.getByRole('textbox', { name: SIZE })

    await typeNumber(user, region, SIZE, '130')
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(savedEmphasis(saves)[0].default?.size_factor).toBe(1.3)

    // A size past 400% is kept at 400%, and one under 25% at 25%.
    await typeNumber(user, region, SIZE, '900')
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(savedEmphasis(saves)[1].default?.size_factor).toBe(4)
    await waitFor(() => expect(size).toHaveValue('400'))
    await typeNumber(user, region, SIZE, '10')
    await waitFor(() => expect(saves).toHaveLength(3))
    expect(savedEmphasis(saves)[2].default?.size_factor).toBe(0.25)
    await waitFor(() => expect(size).toHaveValue('25'))

    // The same size again saves nothing.
    await typeNumber(user, region, SIZE, '25')
    expect(saves).toHaveLength(3)

    await typeNumber(user, region, WEIGHT, '5000')
    await waitFor(() => expect(saves).toHaveLength(4))
    expect(savedEmphasis(saves)[3].default?.weight).toBe(1000)
    await typeNumber(user, region, WEIGHT, '650.4')
    await waitFor(() => expect(saves).toHaveLength(5))
    expect(savedEmphasis(saves)[4].default?.weight).toBe(650)
  })

  it('saves the colour and both outlines of the default, each at once', async () => {
    const { saves } = settingsDesktop()
    const user = userEvent.setup()
    renderTypesetting()
    const region = await card()

    await pickColour(user, region, 'Emphasis color', 'FF0000')
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(savedEmphasis(saves)[0].default?.fill).toEqual([1, 0, 0, 1])
    expect(region.getByText('#FF0000')).toBeInTheDocument()

    await pickColour(user, region, 'Emphasis outline color', 'FFFF00')
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(savedEmphasis(saves)[1].default?.stroke).toEqual([1, 1, 0, 1])

    await typeNumber(user, region, 'Emphasis outline width', '3.5')
    await waitFor(() => expect(saves).toHaveLength(3))
    expect(savedEmphasis(saves)[2].default?.stroke_width).toBe(3.5)

    // The colour the well shows while none is set is saved too.
    await pickColour(user, region, 'Emphasis second outline color', 'FFFFFF')
    await waitFor(() => expect(saves).toHaveLength(4))
    expect(savedEmphasis(saves)[3].default?.outer_stroke).toEqual([1, 1, 1, 1])

    await typeNumber(user, region, 'Emphasis second outline width', '120')
    await waitFor(() => expect(saves).toHaveLength(5))
    expect(savedEmphasis(saves)[4].default).toEqual(
      style({
        weight: 700,
        size_factor: 1.15,
        fill: [1, 0, 0, 1],
        stroke: [1, 1, 0, 1],
        stroke_width: 3.5,
        outer_stroke: [1, 1, 1, 1],
        outer_stroke_width: 100,
      }),
    )
  })

  it('clears a value back to the box style, and a category back to the default', async () => {
    const { file, saves } = settingsDesktop()
    file.preferences['booharu-emphasis'] = {
      ai: true,
      default: style({
        family: 'Anime Ace',
        weight: 700,
        italic: true,
        size_factor: 1.15,
        fill: [1, 0, 0, 1],
      }),
      categories: { shout: style({ weight: 900, stroke_width: 2 }) },
    }
    const user = userEvent.setup()
    renderTypesetting(fontFamilies('Anime Ace'))
    const region = await card()

    await typeNumber(user, region, WEIGHT, '')
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(savedEmphasis(saves)[0].default?.weight).toBeNull()
    expect(region.getByRole('textbox', { name: WEIGHT })).toHaveValue('')
    expect(region.getByRole('textbox', { name: WEIGHT })).toHaveAttribute(
      'placeholder',
      'Use box style',
    )

    await user.click(region.getByRole('button', { name: 'Clear emphasis color' }))
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(savedEmphasis(saves)[1].default?.fill).toBeNull()
    expect(region.getByRole('button', { name: 'Clear emphasis color' })).toBeDisabled()

    await choose(user, region, 'Emphasis italic', 'Use box style')
    await waitFor(() => expect(saves).toHaveLength(3))
    expect(savedEmphasis(saves)[2].default?.italic).toBeNull()

    await user.click(region.getByRole('button', { name: 'Emphasis font' }))
    await user.click(await screen.findByRole('option', { name: 'Use box style' }))
    await waitFor(() => expect(saves).toHaveLength(4))
    expect(savedEmphasis(saves)[3].default).toEqual(style({ size_factor: 1.15 }))

    // A category shows only its own values, the rest the default's.
    await choose(user, region, 'Emphasis category', 'Shout')
    expect(region.getByRole('textbox', { name: WEIGHT })).toHaveValue('900')
    expect(region.getByRole('textbox', { name: SIZE })).toHaveValue('')
    expect(region.getByRole('textbox', { name: SIZE })).toHaveAttribute(
      'placeholder',
      'Same as default',
    )
    expect(region.getByRole('button', { name: 'Emphasis font' })).toHaveTextContent(
      'Same as default',
    )

    await typeNumber(user, region, WEIGHT, '')
    await waitFor(() => expect(saves).toHaveLength(5))
    expect(savedEmphasis(saves)[4].categories).toEqual({ shout: style({ stroke_width: 2 }) })

    // Its last value cleared, the category's entry goes, as the file takes none setting nothing.
    await typeNumber(user, region, 'Emphasis outline width', '')
    await waitFor(() => expect(saves).toHaveLength(6))
    expect(savedEmphasis(saves)[5].categories).toEqual({})
    expect(
      region.getByRole('button', { name: 'Use default emphasis for this category' }),
    ).toBeDisabled()
  })

  it("gives a category its own value, the rest following the default's, and takes them away", async () => {
    const { file, saves } = settingsDesktop()
    file.preferences['booharu-emphasis'].default = style({ weight: 700, fill: [0, 0, 1, 1] })
    const user = userEvent.setup()
    renderTypesetting()
    const region = await card()

    expect(
      region.queryByRole('button', { name: 'Use default emphasis for this category' }),
    ).not.toBeInTheDocument()
    await choose(user, region, 'Emphasis category', 'Whisper')
    const reset = region.getByRole('button', { name: 'Use default emphasis for this category' })
    expect(reset).toBeDisabled()
    expect(region.getByRole('textbox', { name: WEIGHT })).toHaveValue('')
    // A colour the category leaves unset shows the default's.
    const well = region.getByRole('button', { name: 'Emphasis color' })
    expect(well.querySelector('span')).toHaveStyle({ backgroundColor: '#0000FF' })
    expect(region.getByRole('button', { name: 'Clear emphasis color' })).toBeDisabled()

    await typeNumber(user, region, SIZE, '90')
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(savedEmphasis(saves)[0]).toEqual({
      ai: true,
      default: style({ weight: 700, fill: [0, 0, 1, 1] }),
      categories: { whisper: style({ size_factor: 0.9 }) },
    })
    await waitFor(() => expect(reset).toBeEnabled())

    await pickColour(user, region, 'Emphasis color', '00FF00')
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(savedEmphasis(saves)[1].categories).toEqual({
      whisper: style({ size_factor: 0.9, fill: [0, 1, 0, 1] }),
    })

    // The default's own values stay as they are.
    await choose(user, region, 'Emphasis category', 'Default emphasis')
    expect(region.getByRole('textbox', { name: SIZE })).toHaveValue('')
    expect(region.getByRole('textbox', { name: WEIGHT })).toHaveValue('700')

    await choose(user, region, 'Emphasis category', 'Whisper')
    await user.click(region.getByRole('button', { name: 'Use default emphasis for this category' }))
    await waitFor(() => expect(saves).toHaveLength(3))
    expect(savedEmphasis(saves)[2].categories).toEqual({})
    await waitFor(() => expect(region.getByRole('textbox', { name: SIZE })).toHaveValue(''))
  })

  it('reads the file again after a save it refuses, and shows what it holds', async () => {
    const { file, save } = settingsDesktop()
    const read = vi.mocked(commands.booharuGetPreferences)
    // The desktop refuses a default setting nothing, as booharu-core does.
    file.preferences['booharu-emphasis'].default = style({ weight: 700 })
    const user = userEvent.setup()
    renderTypesetting()
    const region = await card()
    const weight = region.getByRole('textbox', { name: WEIGHT })
    save.mockImplementationOnce(async () => {
      // Another writer changes the file after the save read it, and the desktop refuses the save.
      file.preferences['booharu-emphasis'].ai = false
      throw 'the default emphasis sets nothing: give it at least one value'
    })
    const reads = read.mock.calls.length

    await user.clear(weight)
    await user.tab()
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1))
    expect(save.mock.calls[0][0]['booharu-emphasis'].default).toEqual(style({}))

    // The save's own read, then the one after the refusal, which alone sees the other writer's.
    await waitFor(() => expect(read.mock.calls.length).toBe(reads + 2))
    await waitFor(() => expect(weight).toHaveValue('700'))
    await waitFor(() =>
      expect(
        region.getByRole('checkbox', { name: 'Let the AI emphasize words' }),
      ).not.toBeChecked(),
    )
    expect(file.preferences['booharu-emphasis'].default).toEqual(style({ weight: 700 }))
  })
})
