import { fireEvent, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { favoritesDesktop, fontFamilies, highlighted, listed, renderPicker, search } from './fonts'

describe('Booharu font picker keys', () => {
  it('moves through the fonts with Up and Down, wrapping around, and picks one with Enter', async () => {
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Komika Text' })
    await user.click(screen.getByRole('button', { name: 'Font' }))

    // The arrows start from the font in use, the one option they mark.
    expect(search()).toHaveFocus()
    expect(highlighted()).toHaveAccessibleName('Komika Text')
    expect(highlighted()).toHaveAttribute('data-active', 'true')
    expect(
      screen.getAllByRole('option').filter((option) => option.dataset.active === 'true'),
    ).toHaveLength(1)

    await user.keyboard('{ArrowDown}')
    expect(highlighted()).toHaveAccessibleName('Wild Words')
    await user.keyboard('{ArrowDown}')
    expect(highlighted()).toHaveAccessibleName('Anime Ace')
    await user.keyboard('{ArrowUp}{ArrowUp}{ArrowUp}')
    expect(highlighted()).toHaveAccessibleName('CCMeanwhile')
    expect(onChange).not.toHaveBeenCalled()

    await user.keyboard('{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('CCMeanwhile')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
  })

  it('starts again from the first match of a search, and sums up the matches and the font it is on', async () => {
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Komika Text' })
    await user.click(screen.getByRole('button', { name: 'Font' }))

    await user.type(search(), 'w')
    expect(highlighted()).toHaveAccessibleName('CCMeanwhile')
    const summary = screen.getByRole('status')
    expect(summary).toHaveTextContent('2 fonts')
    // The font it is on, its name set in its own face.
    expect(await within(summary).findByRole('img', { name: 'CCMeanwhile' })).toBeInTheDocument()

    await user.keyboard('{ArrowDown}')
    expect(await within(summary).findByRole('img', { name: 'Wild Words' })).toBeInTheDocument()
    await user.keyboard('{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Wild Words')
  })

  it('leaves Enter and the arrows to an IME while it composes', async () => {
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Komika Text' })
    await user.click(screen.getByRole('button', { name: 'Font' }))

    fireEvent.keyDown(search(), { key: 'ArrowDown', isComposing: true })
    expect(highlighted()).toHaveAccessibleName('Komika Text')
    fireEvent.keyDown(search(), { key: 'Enter', isComposing: true })
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

    it('scrolls to the font the arrows move to', async () => {
      layOut(40 * 38)
      const user = userEvent.setup()
      renderPicker({ families: fontFamilies(...names) })
      await user.click(screen.getByRole('button', { name: 'Font' }))
      expect(highlighted()).toHaveAccessibleName('Font 1')

      await user.keyboard('{ArrowUp}')
      expect(scrollTo).toHaveBeenLastCalledWith(expect.objectContaining({ top: 40 * 38 - 248 }))
      expect(highlighted()).toHaveAccessibleName('Font 40')
    })

    it('shows the heading above the first font when the arrows come around to it', async () => {
      layOut(24 + 38 + 24 + 40 * 38)
      favoritesDesktop(['Font 2'])
      const user = userEvent.setup()
      renderPicker({ families: fontFamilies(...names) }, { booharu: true })
      await user.click(screen.getByRole('button', { name: 'Font' }))
      await screen.findByText('Favorites')
      await user.keyboard('{ArrowUp}')
      expect(highlighted()).toHaveAccessibleName('Font 40')

      await user.keyboard('{ArrowDown}')
      expect(highlighted()).toBe(screen.getAllByRole('option', { name: 'Font 2' })[0])
      expect(scrollTo).toHaveBeenLastCalledWith(expect.objectContaining({ top: 0 }))
    })
  })

  it("still picks with the pointer and searches PostScript names, as Koharu's does", async () => {
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Komika Text' })
    await user.click(screen.getByRole('button', { name: 'Font' }))

    expect(screen.getByRole('option', { name: 'Komika Text' })).toHaveAttribute(
      'aria-selected',
      'true',
    )
    expect(screen.getByRole('status')).toHaveTextContent('4 fonts')
    // Without Booharu's props, none of its favourites.
    expect(listed()).toEqual(['Anime Ace', 'CCMeanwhile', 'Komika Text', 'Wild Words'])
    expect(screen.queryByRole('button', { name: 'Favorites only' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /favorites$/ })).not.toBeInTheDocument()

    await user.type(search(), 'WildWords')
    expect(listed()).toEqual(['Wild Words'])
    await user.clear(search())
    await user.type(search(), 'missing family')
    expect(screen.getByText('No fonts match this search')).toBeInTheDocument()
    expect(highlighted()).toBeNull()

    await user.clear(search())
    await user.click(screen.getByRole('option', { name: 'Anime Ace' }))
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Anime Ace')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
  })
})
