import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { TypeBar } from '@/components/editor/TypeBar'
import { TypesettingPreferences } from '@/components/preferences/TypesettingPreferences'
import { saveBooharuSection } from '@/lib/booharu/preferences'
import { fontsKey, pageKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Page } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import {
  comicFonts,
  favoritesDesktop,
  highlighted,
  listed,
  openPicker,
  renderPicker,
  search,
} from './fonts'
import { settingsDesktop } from './settings'

// Koharu's picker stars a family with the star on its row and lists the favourites alone with Show
// favorite fonts; Booharu keeps the stars in the settings file, where the agent reads and stars them
// too, and pins the favourites above every family under their headings.
describe('Booharu font favourites', () => {
  it('pins the favourites above all fonts, each section under its heading', async () => {
    favoritesDesktop(['Wild Words'])
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Komika Text' }, { booharu: true })
    await openPicker(user)
    await screen.findByText('Favorites')

    expect(listed()).toEqual([
      'Wild Words',
      'Anime Ace',
      'CCMeanwhile',
      'Komika Text',
      'Wild Words',
    ])
    expect(screen.getByText('4 fonts')).toBeInTheDocument()
    // Each heading over the first row of its section, which sits below it.
    const [pinned, own] = screen.getAllByRole('option', { name: 'Wild Words' })
    expect(screen.getByText('Favorites')).toHaveStyle({ transform: 'translateY(0px)' })
    expect(pinned).toHaveStyle({ transform: 'translateY(24px)' })
    expect(screen.getByText('All fonts')).toHaveStyle({ transform: 'translateY(62px)' })
    expect(screen.getByRole('option', { name: 'Anime Ace' })).toHaveStyle({
      transform: 'translateY(86px)',
    })
    // Starred in both places.
    expect(within(pinned!).getByRole('button', { pressed: true })).toHaveAccessibleName(
      'Remove Wild Words from favorites',
    )
    expect(within(own!).getByRole('button', { pressed: true })).toHaveAccessibleName(
      'Remove Wild Words from favorites',
    )

    // From the font in use the arrows reach the pinned copy, which Enter picks.
    expect(highlighted()).toHaveAccessibleName('Komika Text')
    await user.keyboard('{ArrowUp}{ArrowUp}{ArrowUp}')
    expect(highlighted()).toBe(pinned)
    await user.keyboard('{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Wild Words')
  })

  it('stars a font through the desktop at once, and takes the star back when that fails', async () => {
    const { set } = favoritesDesktop()
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Komika Text' }, { booharu: true })
    await openPicker(user)

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
    await openPicker(user)

    const stars = await screen.findAllByRole('button', { name: 'Remove Wild Words from favorites' })
    await user.click(stars[0]!)
    expect(set).toHaveBeenCalledExactlyOnceWith('wild  words', false)
    await waitFor(() => expect(screen.queryByText('Favorites')).not.toBeInTheDocument())
    expect(file.favorites).toEqual([])
  })

  // Show favorite fonts stays on for the next test, so this one turns it off again at its end.
  it("lists Booharu's favourites alone with Koharu's Show favorite fonts, keeping none in the browser", async () => {
    favoritesDesktop()
    const user = userEvent.setup()
    renderPicker({}, { booharu: true })
    await openPicker(user)

    const only = screen.getByRole('button', { name: 'Show favorite fonts' })
    expect(only).toHaveAttribute('aria-pressed', 'false')
    await user.click(only)
    expect(only).toHaveAttribute('aria-pressed', 'true')
    expect(listed()).toEqual([])
    expect(screen.getByText('No favorite fonts yet')).toBeInTheDocument()
    expect(search()).toHaveFocus()

    await user.click(only)
    await user.click(screen.getByRole('button', { name: 'Add Komika Text to favorites' }))
    await user.click(only)
    await waitFor(() => expect(listed()).toEqual(['Komika Text']))
    // Koharu's list of them, with no headings.
    expect(screen.queryByText('Favorites')).not.toBeInTheDocument()
    expect(screen.queryByText('All fonts')).not.toBeInTheDocument()
    expect(screen.getByText('1 font')).toBeInTheDocument()
    await user.type(search(), 'Anime')
    expect(screen.getByText('No fonts match this search')).toBeInTheDocument()

    // Reopened, the picker still shows only the favourites.
    await user.keyboard('{Escape}')
    await waitFor(() => expect(screen.queryByRole('listbox')).not.toBeInTheDocument())
    await openPicker(user)
    expect(screen.getByRole('button', { name: 'Show favorite fonts' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    expect(listed()).toEqual(['Komika Text'])

    // The star went to the settings file, and none to Koharu's store in the browser profile.
    const kept = JSON.parse(localStorage.getItem('favorite-fonts') ?? '{}') as {
      state?: { favorites?: string[] }
    }
    expect(kept.state?.favorites).toEqual([])

    await user.click(screen.getByRole('button', { name: 'Show favorite fonts' }))
    expect(screen.getByRole('button', { name: 'Show favorite fonts' })).toHaveAttribute(
      'aria-pressed',
      'false',
    )
  })

  it("shares the favourites between Typesetting's pickers and the TypeBar's", async () => {
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
    await screen.findByRole('combobox', { name: 'Add a fallback' })
    await waitFor(() =>
      expect(screen.getByRole('combobox', { name: 'Add a fallback' })).toBeEnabled(),
    )
    await openPicker(user, 'Add a fallback')
    await user.click(await screen.findByRole('button', { name: 'Add Komika Text to favorites' }))
    await screen.findByText('Favorites')
    await user.keyboard('{Escape}')
    await waitFor(() => expect(screen.queryByRole('listbox')).not.toBeInTheDocument())
    await openPicker(user, 'Primary')
    expect(await screen.findByText('Favorites')).toBeInTheDocument()
    expect(listed()[0]).toBe('Komika Text')
    await user.keyboard('{Escape}')
    await waitFor(() => expect(screen.queryByRole('listbox')).not.toBeInTheDocument())

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
    await openPicker(user)

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
