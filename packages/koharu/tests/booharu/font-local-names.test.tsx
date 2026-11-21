import { cleanup, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { commands } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import {
  comicFonts,
  favoritesDesktop,
  fontFamilies,
  highlighted,
  listed,
  openPicker,
  renderPicker,
  search,
} from './fonts'

/** The comic fonts and two families with names in other languages, in name order. */
const families = [...comicFonts, ...fontFamilies('SimSun', 'Yu Gothic')].sort((left, right) =>
  left.name.localeCompare(right.name),
)

const localNames = { SimSun: ['宋体', '宋體'], 'Yu Gothic': ['游ゴシック'] }

describe('Booharu local font names', () => {
  it('finds a family by its names in other languages, and shows the first muted after its name', async () => {
    favoritesDesktop()
    vi.spyOn(commands, 'booharuGetFontLocalNames').mockResolvedValue(localNames)
    const user = userEvent.setup()
    const { onChange } = renderPicker({ families }, { booharu: true })
    await openPicker(user)

    const yuGothic = screen.getByRole('option', { name: 'Yu Gothic' })
    const local = await within(yuGothic).findByText('游ゴシック')
    expect(local).toHaveClass('opacity-70')
    expect(local.parentElement).toHaveTextContent('Yu Gothic游ゴシック')
    // Only the first of a family's names shows, but a search finds it by any.
    expect(screen.getByText('宋体')).toBeInTheDocument()
    expect(screen.queryByText('宋體')).not.toBeInTheDocument()
    await user.type(search(), '宋體')
    expect(listed()).toEqual(['SimSun'])

    await user.clear(search())
    await user.type(search(), '游ゴシック')
    expect(listed()).toEqual(['Yu Gothic'])
    await user.keyboard('{ArrowDown}')
    expect(highlighted()).toHaveAccessibleName('Yu Gothic')
    await user.keyboard('{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Yu Gothic')
  })

  it('works without the names while they are on their way', async () => {
    favoritesDesktop()
    let answer: (names: typeof localNames) => void = () => undefined
    vi.spyOn(commands, 'booharuGetFontLocalNames').mockImplementation(
      () => new Promise((resolve) => (answer = resolve)),
    )
    const user = userEvent.setup()
    const { onChange } = renderPicker({ families }, { booharu: true })
    await openPicker(user)

    expect(listed()).toHaveLength(6)
    await user.type(search(), 'Yu')
    expect(listed()).toEqual(['Yu Gothic'])
    await user.clear(search())
    await user.type(search(), '游')
    expect(screen.getByText('No fonts match this search')).toBeInTheDocument()

    answer(localNames)
    await waitFor(() => expect(listed()).toEqual(['Yu Gothic']))
    expect(screen.getByText('游ゴシック')).toHaveClass('opacity-70')
    await user.keyboard('{ArrowDown}{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Yu Gothic')
  })

  it('reads them again for the next picker, as a family downloaded since has names', async () => {
    favoritesDesktop()
    const read = vi
      .spyOn(commands, 'booharuGetFontLocalNames')
      .mockResolvedValueOnce({ SimSun: localNames.SimSun })
      .mockResolvedValueOnce(localNames)
    const user = userEvent.setup()
    renderPicker({ families }, { booharu: true })
    await openPicker(user)
    expect(await screen.findByText('宋体')).toBeInTheDocument()
    expect(screen.queryByText('游ゴシック')).not.toBeInTheDocument()

    cleanup()
    renderPicker({ families }, { booharu: true })
    await openPicker(user)
    expect(await screen.findByText('游ゴシック')).toBeInTheDocument()
    await user.type(search(), '游ゴシック')
    expect(listed()).toEqual(['Yu Gothic'])
    expect(read).toHaveBeenCalledTimes(2)
  })

  it("shows nothing when the names can't be read, and the next picker reads them again", async () => {
    favoritesDesktop()
    const read = vi
      .spyOn(commands, 'booharuGetFontLocalNames')
      .mockRejectedValueOnce('no font names')
      .mockResolvedValueOnce(localNames)
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    const { onChange } = renderPicker({ families }, { booharu: true })
    await openPicker(user)

    await waitFor(() => expect(read).toHaveBeenCalledOnce())
    await user.type(search(), 'Komika')
    await user.keyboard('{ArrowDown}{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Komika Text')
    expect(shown).not.toHaveBeenCalled()

    cleanup()
    renderPicker({ families }, { booharu: true })
    await openPicker(user)
    expect(await screen.findByText('游ゴシック')).toBeInTheDocument()
    expect(read).toHaveBeenCalledTimes(2)
  })
})
