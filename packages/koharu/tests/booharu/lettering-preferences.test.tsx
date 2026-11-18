import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { TypesettingPreferences } from '@/components/preferences/TypesettingPreferences'
import { type ProjectView, projectKey, queryClient } from '@/lib/queries'
import { commands, type FontFamily, type LetteringSettings } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { fontFamilies } from './fonts'
import { builtInLettering, settingsDesktop } from './settings'

const APPLY = 'Apply to the open project'
const SWITCH = 'Pick fonts automatically'

/** A family neither bold nor italic. */
const plain = (family: string) => ({ family, bold: false, italic: false })

/** Settings > Typesetting, its first Preferred font, the default font, CCWildWords, and `fonts`
 * the families Koharu lists, each preview a blank image. */
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

/** The Lettering categories card, once it shows the settings file. */
async function card() {
  const region = within(await screen.findByRole('region', { name: 'Lettering categories' }))
  await waitFor(() => expect(region.getByRole('switch', { name: SWITCH })).toBeEnabled())
  return region
}

/** The lettering category picker's search line, once the list opens and takes the focus. */
async function categorySearch() {
  const search = await screen.findByRole('combobox', { name: 'Search categories' })
  await waitFor(() => expect(search).toHaveFocus())
  return search
}

/** The categories the open list shows, by name, in order. */
function categoriesListed() {
  return screen.queryAllByRole('option').map((option) => option.textContent)
}

/** The fonts the open font list shows, by name, in order. */
async function fontsListed() {
  const list = await screen.findByRole('listbox', { name: 'Fonts' })
  return within(list)
    .queryAllByRole('option')
    .map((option) => option.getAttribute('aria-label'))
}

/** What a font picker's button shows: the chosen family's preview, named by it, or a label. */
function showing(button: HTMLElement) {
  return within(button).queryByRole('img')?.getAttribute('alt') ?? button.textContent
}

/** The `[booharu-lettering]` each save sent. */
function saved(saves: ReturnType<typeof settingsDesktop>['saves']) {
  return saves.map((preferences) => preferences['booharu-lettering'])
}

describe('Booharu Lettering categories', () => {
  it('starts with the switch off and Normal Dialogue in the default font', async () => {
    settingsDesktop()
    renderTypesetting()
    const region = await card()

    expect(region.getByRole('switch', { name: SWITCH })).not.toBeChecked()
    expect(region.getByRole('combobox', { name: 'Lettering category' })).toHaveTextContent(
      'Normal Dialogue',
    )
    expect(showing(region.getByRole('button', { name: 'Normal Dialogue: Font' }))).toBe(
      'Same as the default font',
    )
    expect(region.getByText('Also used for round bubbles.')).toBeInTheDocument()
    expect(region.getByRole('button', { name: 'Normal Dialogue: Bold' })).toHaveAttribute(
      'aria-pressed',
      'false',
    )
    expect(region.getByText(/^None yet/)).toBeInTheDocument()
    // No project is open, and the switch is off besides.
    expect(region.getByRole('button', { name: APPLY })).toBeDisabled()
    expect(
      region.getByText('Turn on “Pick fonts automatically” to give them their category fonts.'),
    ).toBeInTheDocument()
  })

  it('saves only its own settings, over what the file holds just before', async () => {
    const lettering = {
      ...builtInLettering,
      default: { fill: [200, 30, 30, 255], stroke: [255, 255, 255, 255], stroke_width: 3 },
      detect_colors: false,
      categories: { shout: plain('Komika Text') },
      small_text: 0.02,
      model_confidence: 0.75,
    } satisfies LetteringSettings
    const { file, saves } = settingsDesktop(lettering)
    const user = userEvent.setup()
    renderTypesetting()
    const region = await card()

    // Another writer, such as the agent's settings tool, changes the section meanwhile.
    const section = file.preferences['booharu-lettering']
    section.detect_colors = true
    section.categories = { ...section.categories, moan: plain('Anime Ace') }

    await user.click(region.getByRole('switch', { name: SWITCH }))
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saved(saves)[0]).toEqual({
      ...lettering,
      detect_colors: true,
      auto: true,
      categories: { shout: plain('Komika Text'), moan: plain('Anime Ace') },
    })
    expect(region.getByRole('switch', { name: SWITCH })).toBeChecked()

    // Bold on Normal Dialogue, which has no font of its own, keeps the default font.
    await user.click(region.getByRole('button', { name: 'Normal Dialogue: Bold' }))
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(saved(saves)[1]).toEqual({
      ...lettering,
      detect_colors: true,
      auto: true,
      categories: {
        shout: plain('Komika Text'),
        moan: plain('Anime Ace'),
        normal_dialogue: { family: null, bold: true, italic: false },
      },
    })
    expect(region.getByRole('button', { name: 'Normal Dialogue: Bold' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
  })

  it('finds a category as you type, and shows its font, bold and italic', async () => {
    settingsDesktop({
      ...builtInLettering,
      categories: { shout: { family: 'Komika Text', bold: true, italic: false } },
    })
    const user = userEvent.setup()
    renderTypesetting(fontFamilies('Anime Ace', 'CCWildWords', 'Komika Text'))
    const region = await card()
    const picker = region.getByRole('combobox', { name: 'Lettering category' })

    await user.click(picker)
    await categorySearch()
    expect(categoriesListed()).toHaveLength(18)
    await user.keyboard('sh')
    expect(categoriesListed()).toEqual(['Shout', 'Weak/Panic Shout', 'Shy / Hesitant'])
    // The first match is marked, so Enter picks it.
    await user.keyboard('{Enter}')
    await waitFor(() => expect(picker).toHaveTextContent('Shout'))
    await waitFor(() => expect(picker).toHaveFocus())

    expect(showing(region.getByRole('button', { name: 'Shout: Font' }))).toBe('Komika Text')
    expect(region.getByRole('button', { name: 'Shout: Bold' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    expect(region.getByRole('button', { name: 'Shout: Italic' })).toHaveAttribute(
      'aria-pressed',
      'false',
    )
    expect(region.getByText('Also used for spiky bubbles.')).toBeInTheDocument()

    // The arrows move through the matches.
    await user.keyboard('{ArrowDown}')
    await categorySearch()
    await user.keyboard('whis{ArrowDown}{ArrowUp}{Enter}')
    await waitFor(() => expect(picker).toHaveTextContent('Whisper'))
    expect(showing(region.getByRole('button', { name: 'Whisper: Font' }))).toBe(
      'Same as Normal Dialogue',
    )
    expect(region.queryByText(/^Also used for/)).toBeNull()

    // A search that matches nothing says so.
    await user.click(picker)
    await categorySearch()
    await user.keyboard('zzz')
    expect(categoriesListed()).toEqual([])
    expect(screen.getByText('No categories match')).toBeInTheDocument()
  })

  it('clears a category with the font picker’s leading row, keeping the others', async () => {
    const { saves } = settingsDesktop({
      ...builtInLettering,
      auto: true,
      categories: {
        whisper: { family: 'Anime Ace', bold: false, italic: true },
        shout: plain('Komika Text'),
      },
    })
    const user = userEvent.setup()
    renderTypesetting(fontFamilies('Anime Ace', 'CCWildWords', 'Komika Text'))
    const region = await card()

    // The categories with their own font, in Settings' order, each a button that picks it.
    const own = within(region.getByRole('list', { name: 'Categories with their own font' }))
    expect(own.getAllByRole('button').map((button) => button.textContent)).toEqual([
      'Shout Komika Text',
      'Whisper Anime Ace · Italic',
    ])
    await user.click(own.getByRole('button', { name: 'Whisper Anime Ace · Italic' }))
    expect(region.getByRole('combobox', { name: 'Lettering category' })).toHaveTextContent(
      'Whisper',
    )
    expect(own.getByRole('button', { name: /^Whisper/ })).toHaveAttribute('aria-pressed', 'true')

    const font = region.getByRole('button', { name: 'Whisper: Font' })
    expect(showing(font)).toBe('Anime Ace')
    await user.click(font)
    expect(await fontsListed()).toEqual([
      'Same as Normal Dialogue',
      'Anime Ace',
      'CCWildWords',
      'Komika Text',
    ])
    await user.click(screen.getByRole('option', { name: 'Same as Normal Dialogue' }))

    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saved(saves)[0]?.categories).toEqual({ shout: plain('Komika Text') })
    await waitFor(() => expect(showing(font)).toBe('Same as Normal Dialogue'))
    expect(own.getAllByRole('button').map((button) => button.textContent)).toEqual([
      'Shout Komika Text',
    ])

    // Picking a family gives the category its own font again, as italic as it was lettered.
    await user.click(font)
    await user.click(await screen.findByRole('option', { name: 'CCWildWords' }))
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(saved(saves)[1]?.categories).toEqual({
      shout: plain('Komika Text'),
      whisper: plain('CCWildWords'),
    })
  })

  it('suggests starting fonts for the categories without one, and says which', async () => {
    const { saves } = settingsDesktop({
      ...builtInLettering,
      categories: { normal_dialogue: plain('Komika Text') },
    })
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    renderTypesetting(
      fontFamilies('Anime Ace 2.0 BB', 'CCWildWords', 'Hogfish Demo', 'Komika Text'),
    )
    const region = await card()

    const suggest = region.getByRole('button', { name: 'Suggest fonts' })
    await waitFor(() => expect(suggest).toBeEnabled())
    await user.click(suggest)
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saved(saves)[0]?.categories).toEqual({
      normal_dialogue: plain('Komika Text'),
      thoughts: { family: 'Anime Ace 2.0 BB', bold: false, italic: true },
      floating: plain('Hogfish Demo'),
    })
    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'success',
          title: 'Suggested fonts for 2 categories',
          description: 'Thoughts and Floating',
        }),
      ),
    )

    // Asked again, it has nothing to fill, and saves nothing.
    await user.click(suggest)
    expect(shown).toHaveBeenLastCalledWith(
      expect.objectContaining({
        type: 'info',
        title: expect.stringMatching(/^Nothing to suggest/),
      }),
    )
    expect(saves).toHaveLength(1)
  })

  it('applies the category fonts through Apply once the switch is on', async () => {
    const { file } = settingsDesktop()
    const project: ProjectView = {
      name: 'Demo',
      page: 'page',
      pages: [{ id: 'page', size: { width: 800, height: 1200 }, background: null }],
      history: { steps: [], current: 0 },
      revisions: {},
    }
    queryClient.setQueryData(projectKey, project)
    const seen: Array<{ auto?: boolean; width?: number }> = []
    const count = vi
      .spyOn(commands, 'booharuCountTextStyleTargets')
      .mockImplementation(async () => {
        const section = file.preferences['booharu-lettering']
        seen.push({ auto: section.auto, width: section.default?.stroke_width })
        return { changed: 3, kept_hand_edited: 0, untranslated: 0 }
      })
    const user = userEvent.setup()
    renderTypesetting()
    const region = await card()

    const apply = region.getByRole('button', { name: APPLY })
    expect(apply).toBeDisabled()
    await user.click(region.getByRole('switch', { name: SWITCH }))
    await waitFor(() => expect(apply).toBeEnabled())
    const wording =
      "Give them their category fonts, and the default colors when “Use the original lettering's colors” is off."
    expect(region.getByText(wording)).toBeInTheDocument()
    // The default text style's own Apply says so too.
    const defaults = within(screen.getByRole('region', { name: 'Default text style' }))
    expect(defaults.getByText(wording)).toBeInTheDocument()

    // A default text style change made just before is saved first, as the section's own Apply does.
    await user.click(defaults.getByRole('button', { name: 'Increase outline width' }))
    await user.click(apply)
    const dialog = within(await screen.findByRole('alertdialog'))
    expect(dialog.getByText('3 boxes will be restyled.')).toBeInTheDocument()
    expect(count).toHaveBeenCalledExactlyOnceWith({ kind: 'project' }, false)
    expect(seen).toEqual([{ auto: true, width: 2 }])
  })
})
