import { fireEvent, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import {
  favoritesDesktop,
  fontFamilies,
  highlighted,
  listed,
  openPicker,
  renderPicker,
  search,
} from './fonts'

const SAME = 'Same as Normal Dialogue'

// Koharu's picker moves through its list with the arrows and picks with Enter itself; these test the
// rows Booharu adds to it, the leading row and the pinned favourites, and their headings.
describe("Booharu's rows in Koharu's font picker, by keyboard", () => {
  it('reaches the leading row and the pinned favourites with the arrows, and Enter picks them', async () => {
    favoritesDesktop(['Wild Words'])
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Komika Text', noneLabel: SAME })
    await openPicker(user)
    await screen.findByText('Favorites')

    expect(listed()).toEqual([
      SAME,
      'Wild Words',
      'Anime Ace',
      'CCMeanwhile',
      'Komika Text',
      'Wild Words',
    ])
    // The families, the leading row and the pinned copies not among them.
    expect(screen.getByText('4 fonts')).toBeInTheDocument()

    // From the font in use, under All fonts, round to the leading row and the pinned favourite.
    expect(highlighted()).toHaveAccessibleName('Komika Text')
    await user.keyboard('{ArrowDown}')
    expect(highlighted()).toBe(screen.getAllByRole('option', { name: 'Wild Words' })[1])
    await user.keyboard('{ArrowDown}{ArrowDown}')
    expect(highlighted()).toBe(screen.getByRole('option', { name: SAME }))
    await user.keyboard('{ArrowDown}')
    expect(highlighted()).toBe(screen.getAllByRole('option', { name: 'Wild Words' })[0])
    expect(onChange).not.toHaveBeenCalled()

    await user.keyboard('{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Wild Words')
    await waitFor(() => expect(screen.queryByRole('listbox')).not.toBeInTheDocument())
  })

  it("keeps Koharu's search by PostScript name and pick by pointer, the favourite found twice", async () => {
    favoritesDesktop(['Wild Words'])
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Komika Text', noneLabel: SAME })
    await openPicker(user)
    await screen.findByText('Favorites')

    await user.type(search(), 'WildWords')
    expect(listed()).toEqual(['Wild Words', 'Wild Words'])
    expect(screen.getByText('1 font')).toBeInTheDocument()
    await user.clear(search())
    await user.type(search(), 'missing family')
    expect(screen.getByText('No fonts match this search')).toBeInTheDocument()

    await user.clear(search())
    await user.click(screen.getByRole('option', { name: 'Anime Ace' }))
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Anime Ace')
    await waitFor(() => expect(screen.queryByRole('listbox')).not.toBeInTheDocument())
  })

  it('leaves Enter and the arrows to an IME while it composes, as Base UI does', async () => {
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Komika Text' })
    await openPicker(user)

    // A key pressed while an IME composes comes with key code 229.
    expect(highlighted()).toHaveAccessibleName('Komika Text')
    fireEvent.keyDown(search(), { key: 'ArrowDown', keyCode: 229 })
    expect(highlighted()).toHaveAccessibleName('Komika Text')
    fireEvent.keyDown(search(), { key: 'Enter', keyCode: 229 })
    expect(onChange).not.toHaveBeenCalled()
    expect(screen.getByRole('listbox')).toBeInTheDocument()

    // Once it's done, Enter picks.
    fireEvent.keyDown(search(), { key: 'Enter' })
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Komika Text')
  })

  describe('in a long list', () => {
    // jsdom lays nothing out and doesn't scroll, so the list's viewport is made 248 px high, over
    // rows `height` px high in all (38 px each, 24 px for a heading), and made to scroll when told.
    const scrollTo = vi.fn(function (this: HTMLElement, to?: ScrollToOptions | number, y = 0) {
      const top = typeof to === 'number' ? y : (to?.top ?? 0)
      Object.defineProperty(this, 'scrollTop', { configurable: true, value: top })
      queueMicrotask(() => this.dispatchEvent(new Event('scroll')))
    })
    const names = Array.from({ length: 40 }, (_, index) => `Font ${index + 1}`)
    const layOut = (height: number) => {
      HTMLElement.prototype.scrollTo = scrollTo
      Object.defineProperty(HTMLElement.prototype, 'scrollHeight', {
        configurable: true,
        get: () => height,
      })
      Object.defineProperty(HTMLElement.prototype, 'clientHeight', {
        configurable: true,
        get: () => 248,
      })
    }
    afterEach(() => {
      for (const key of ['scrollTo', 'scrollHeight', 'clientHeight'] as const) {
        delete (HTMLElement.prototype as Partial<HTMLElement>)[key]
      }
    })

    it('shows the heading above the first favourite when the arrows come round to it', async () => {
      const height = 24 + 38 + 24 + 40 * 38
      layOut(height)
      favoritesDesktop(['Font 2'])
      const user = userEvent.setup()
      renderPicker({ families: fontFamilies(...names) })
      await openPicker(user)
      await screen.findByText('Favorites')

      // The last family, at the bottom, both headings above it.
      await user.keyboard('{ArrowUp}')
      expect(highlighted()).toHaveAccessibleName('Font 40')
      expect(scrollTo).toHaveBeenLastCalledWith(expect.objectContaining({ top: height - 248 }))

      // Round through the search line to the pinned favourite, its heading in view above it.
      await user.keyboard('{ArrowDown}')
      expect(highlighted()).toBeNull()
      await user.keyboard('{ArrowDown}')
      expect(highlighted()).toBe(screen.getAllByRole('option', { name: 'Font 2' })[0])
      expect(scrollTo).toHaveBeenLastCalledWith(expect.objectContaining({ top: 0 }))
      expect(await screen.findByText('Favorites')).toBeInTheDocument()
    })
  })
})
