import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { TypeBar } from '@/components/editor/TypeBar'
import { TypesettingPreferences } from '@/components/preferences/TypesettingPreferences'
import { setFavoritesOnly } from '@/lib/booharu/fonts'
import { saveBooharuSection } from '@/lib/booharu/preferences'
import { fontsKey, pageKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Page } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import {
  comicFonts,
  favoritesDesktop,
  fontFamilies,
  highlighted,
  listed,
  renderPicker,
  search,
} from './fonts'
import { settingsDesktop } from './settings'

describe('Booharu font favourites', () => {
  // Favorites only lasts the session, so a test that turns it on leaves it on for the next.
  beforeEach(() => setFavoritesOnly(false))

  it('pins the favourites above all fonts, and the arrows skip the headings', async () => {
    favoritesDesktop(['Wild Words'])
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Komika Text' }, { booharu: true })
    await user.click(screen.getByRole('button', { name: 'Font' }))

    await screen.findByText('Favorites')
    expect(screen.getByText('All fonts')).toBeInTheDocument()
    expect(listed()).toEqual([
      'Wild Words',
      'Anime Ace',
      'CCMeanwhile',
      'Komika Text',
      'Wild Words',
    ])
    expect(screen.getByRole('status')).toHaveTextContent('4 fonts')
    expect(search()).toHaveAttribute('aria-controls', screen.getByRole('listbox').id)

    // From the font in use, under All fonts, around past both headings.
    expect(highlighted()).toHaveAccessibleName('Komika Text')
    await user.keyboard('{ArrowDown}{ArrowDown}')
    expect(highlighted()).toBe(screen.getAllByRole('option', { name: 'Wild Words' })[0])
    await user.keyboard('{ArrowDown}')
    expect(highlighted()).toHaveAccessibleName('Anime Ace')
    await user.keyboard('{ArrowUp}')
    expect(highlighted()).toBe(screen.getAllByRole('option', { name: 'Wild Words' })[0])
    await user.keyboard('{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Wild Words')
  })

  it('keeps the arrows on a favourite whose pinned row goes as it is unstarred', async () => {
    favoritesDesktop(['Komika Text', 'Wild Words'])
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Anime Ace' }, { booharu: true })
    await user.click(screen.getByRole('button', { name: 'Font' }))
    await screen.findByText('Favorites')

    // Up from the font in use, to Wild Words pinned under Favorites.
    await user.keyboard('{ArrowUp}')
    expect(highlighted()).toBe(screen.getAllByRole('option', { name: 'Wild Words' })[0])
    await user.click(
      screen.getAllByRole('button', { name: 'Remove Wild Words from favorites' })[0]!,
    )

    await waitFor(() =>
      expect(screen.getAllByRole('option', { name: 'Wild Words' })).toHaveLength(1),
    )
    expect(highlighted()).toHaveAccessibleName('Wild Words')
    await user.keyboard('{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Wild Words')
  })

  it('stars a font through the desktop at once, and takes the star back when that fails', async () => {
    const { set } = favoritesDesktop()
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Komika Text' }, { booharu: true })
    await user.click(screen.getByRole('button', { name: 'Font' }))

    await user.click(screen.getByRole('button', { name: 'Add Wild Words to favorites' }))
    expect(set).toHaveBeenCalledExactlyOnceWith('Wild Words', true)
    await screen.findByText('Favorites')
    expect(listed()[0]).toBe('Wild Words')
    expect(
      screen.getAllByRole('button', { name: 'Remove Wild Words from favorites' }),
    ).toHaveLength(2)
    // Starring picks nothing, and the search line keeps the keys.
    expect(onChange).not.toHaveBeenCalled()
    expect(search()).toHaveFocus()

    let fail: (reason: string) => void = () => undefined
    set.mockImplementationOnce(() => new Promise((_, reject) => (fail = reject)))
    await user.click(screen.getByRole('button', { name: 'Add Anime Ace to favorites' }))
    // At once, before the desktop answers.
    expect(listed()).toEqual([
      'Anime Ace',
      'Wild Words',
      'Anime Ace',
      'CCMeanwhile',
      'Komika Text',
      'Wild Words',
    ])
    fail('config.toml is read-only')

    await waitFor(() => expect(listed()[1]).toBe('Anime Ace'))
    expect(listed()[0]).toBe('Wild Words')
    expect(screen.getByRole('button', { name: 'Add Anime Ace to favorites' })).toBeInTheDocument()
    expect(shown).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'error', description: 'config.toml is read-only' }),
    )
  })

  it('unstars a favourite by the name the settings file keeps it under', async () => {
    const { set, file } = favoritesDesktop(['wild  words'])
    const user = userEvent.setup()
    renderPicker({}, { booharu: true })
    await user.click(screen.getByRole('button', { name: 'Font' }))

    const stars = await screen.findAllByRole('button', { name: 'Remove Wild Words from favorites' })
    await user.click(stars[0]!)
    expect(set).toHaveBeenCalledExactlyOnceWith('wild  words', false)
    await waitFor(() => expect(screen.queryByText('Favorites')).not.toBeInTheDocument())
    expect(file.favorites).toEqual([])
  })

  it('shows only the favourites, or how to star some while there are none', async () => {
    favoritesDesktop()
    const user = userEvent.setup()
    renderPicker({}, { booharu: true })
    await user.click(screen.getByRole('button', { name: 'Font' }))

    const only = screen.getByRole('button', { name: 'Favorites only' })
    expect(only).toHaveAttribute('aria-pressed', 'false')
    await user.click(only)
    expect(only).toHaveAttribute('aria-pressed', 'true')
    expect(listed()).toEqual([])
    expect(screen.getByText('Star fonts to add them here')).toBeInTheDocument()
    expect(search()).toHaveFocus()

    await user.click(only)
    await user.click(screen.getByRole('button', { name: 'Add Komika Text to favorites' }))
    await user.click(only)
    await waitFor(() => expect(listed()).toEqual(['Komika Text']))
    expect(screen.getByText('Favorites')).toBeInTheDocument()
    expect(screen.queryByText('All fonts')).not.toBeInTheDocument()
    expect(within(screen.getAllByRole('status')[0]!).getByText('1 font')).toBeInTheDocument()

    // A search finds none of them.
    await user.type(search(), 'Anime')
    expect(screen.getByText('No fonts match this search')).toBeInTheDocument()

    // Reopened, the picker still shows only the favourites.
    await user.keyboard('{Escape}')
    await user.click(screen.getByRole('button', { name: 'Font' }))
    expect(screen.getByRole('button', { name: 'Favorites only' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    expect(listed()).toEqual(['Komika Text'])
  })

  it('says when Favorites only hides every font a picker offers, not that a search found none', async () => {
    favoritesDesktop(['Anime Ace'])
    setFavoritesOnly(true)
    const user = userEvent.setup()
    // A picker without the favourite, as Add font family leaves out the families already listed.
    renderPicker({ families: fontFamilies('Komika Text', 'Wild Words') }, { booharu: true })
    await user.click(screen.getByRole('button', { name: 'Font' }))

    expect(
      await screen.findByText(
        'None of your favorites are available here. Turn off Favorites only to see all fonts.',
      ),
    ).toBeInTheDocument()
    expect(screen.queryByText('No fonts match this search')).not.toBeInTheDocument()

    await user.type(search(), 'Wild')
    expect(screen.getByText('No fonts match this search')).toBeInTheDocument()
  })

  it("shares the favourites between Preferred fonts' pickers and the TypeBar's", async () => {
    settingsDesktop()
    favoritesDesktop()
    vi.spyOn(commands, 'listFonts').mockResolvedValue(comicFonts)
    vi.spyOn(commands, 'getFontPreview').mockResolvedValue(new ArrayBuffer(0))
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <TypesettingPreferences value={{ font_families: ['Anime Ace'] }} onChange={() => {}} />
      </QueryClientProvider>,
    )
    await user.click(await screen.findByRole('button', { name: 'Add font family' }))
    await user.click(await screen.findByRole('button', { name: 'Add Komika Text to favorites' }))
    await screen.findByText('Favorites')
    await user.keyboard('{Escape}')
    await user.click(screen.getByRole('button', { name: 'Default font family 1' }))
    expect(await screen.findByText('Favorites')).toBeInTheDocument()
    expect(listed()[0]).toBe('Komika Text')
    await user.keyboard('{Escape}')

    const page: Page = {
      id: 'page',
      size: { width: 800, height: 1200 },
      background: null,
      layers: [
        {
          id: 'text',
          name: 'Text',
          visible: true,
          locked: false,
          opacity: 1,
          blend: 'normal',
          attributes: {},
          kind: 'text',
          content: [{ insert: 'Hello' }],
          style: { chars: { size: 24, family: 'Anime Ace' } },
        },
      ],
    }
    queryClient.setQueryData(pageKey, page)
    queryClient.setQueryData(fontsKey, comicFonts)
    useKoharuStore.setState({
      tool: 'select',
      selectedLayers: ['text'],
      selectionBounds: { x: 100, y: 100, width: 200, height: 80 },
    })
    render(
      <QueryClientProvider client={queryClient}>
        <TypeBar />
      </QueryClientProvider>,
    )
    await user.click(within(screen.getByTestId('type-bar')).getByTestId('type-font-picker'))
    expect(await screen.findByText('Favorites')).toBeInTheDocument()
    expect(listed()[0]).toBe('Komika Text')
  })

  it('stars a font after a settings save on its way, which writes the favourites as it read them', async () => {
    // The desktop keeps the favourites in the settings file, which a save writes whole.
    const { file } = settingsDesktop()
    vi.spyOn(commands, 'booharuGetFontFavorites').mockImplementation(async () => [
      ...(file.preferences['booharu-fonts'].favorites ?? []),
    ])
    const set = vi
      .spyOn(commands, 'booharuSetFontFavorite')
      .mockImplementation(async (family, favorite) => {
        const others = (file.preferences['booharu-fonts'].favorites ?? []).filter(
          (name) => name !== family,
        )
        file.preferences['booharu-fonts'].favorites = favorite ? [...others, family] : others
        return file.preferences['booharu-fonts'].favorites
      })
    // The save's read of the file, answered once the star is given.
    let answer = () => {}
    vi.spyOn(commands, 'booharuGetPreferences').mockImplementationOnce(() => {
      const read = structuredClone(file.preferences)
      return new Promise((resolve) => (answer = () => resolve(read)))
    })
    const user = userEvent.setup()
    renderPicker({}, { booharu: true })
    await user.click(screen.getByRole('button', { name: 'Font' }))

    const saved = saveBooharuSection('booharu-lettering', (section) => ({
      ...section,
      detect_colors: false,
    }))
    await user.click(screen.getByRole('button', { name: 'Add Komika Text to favorites' }))
    expect(listed()[0]).toBe('Komika Text')
    expect(set).not.toHaveBeenCalled()
    answer()
    await saved
    await waitFor(() => expect(set).toHaveBeenCalledExactlyOnceWith('Komika Text', true))
    await waitFor(() =>
      expect(file.preferences['booharu-fonts'].favorites).toEqual(['Komika Text']),
    )
    expect(file.preferences['booharu-lettering'].detect_colors).toBe(false)
  })
})
