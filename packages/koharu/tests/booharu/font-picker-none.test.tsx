import { screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { setFavoritesOnly } from '@/lib/booharu/fonts'

import { favoritesDesktop, highlighted, listed, renderPicker, search } from './fonts'

const SAME = 'Same as Normal Dialogue'

describe('Booharu font picker leading row', () => {
  it('shows an empty value as the leading row, chosen, and reaches it with the arrows', async () => {
    const user = userEvent.setup()
    const { onChange } = renderPicker({ noneLabel: SAME })

    const button = screen.getByRole('button', { name: 'Font' })
    expect(button).toHaveTextContent(SAME)
    await user.click(button)

    // First, before the families, chosen, and where the arrows start.
    expect(listed()).toEqual([SAME, 'Anime Ace', 'CCMeanwhile', 'Komika Text', 'Wild Words'])
    const none = screen.getByRole('option', { name: SAME })
    expect(none).toHaveAttribute('aria-selected', 'true')
    expect(highlighted()).toBe(none)
    // The families count, the leading row not among them, and the summary names it.
    const summary = screen.getByRole('status')
    expect(summary).toHaveTextContent('4 fonts')
    expect(summary).toHaveTextContent(SAME)

    await user.keyboard('{ArrowDown}')
    expect(highlighted()).toHaveAccessibleName('Anime Ace')
    await user.keyboard('{ArrowUp}{ArrowUp}')
    expect(highlighted()).toHaveAccessibleName('Wild Words')
    await user.keyboard('{ArrowDown}')
    expect(highlighted()).toBe(screen.getByRole('option', { name: SAME }))

    await user.keyboard('{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
  })

  it('starts from the family in use, and picking the leading row gives no family', async () => {
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Komika Text', noneLabel: SAME })
    await user.click(screen.getByRole('button', { name: 'Font' }))

    expect(highlighted()).toHaveAccessibleName('Komika Text')
    expect(screen.getByRole('option', { name: SAME })).toHaveAttribute('aria-selected', 'false')
    expect(screen.getByRole('option', { name: 'Komika Text' })).toHaveAttribute(
      'aria-selected',
      'true',
    )

    await user.click(screen.getByRole('option', { name: SAME }))
    expect(onChange).toHaveBeenCalledExactlyOnceWith('')
  })

  it('leaves a search to the families, so Enter picks the first match', async () => {
    const user = userEvent.setup()
    const { onChange } = renderPicker({ noneLabel: SAME })
    await user.click(screen.getByRole('button', { name: 'Font' }))

    await user.type(search(), 'w')
    expect(listed()).toEqual(['CCMeanwhile', 'Wild Words'])
    expect(highlighted()).toHaveAccessibleName('CCMeanwhile')

    // Cleared, the search shows it again, where the arrows start.
    await user.clear(search())
    expect(highlighted()).toBe(screen.getByRole('option', { name: SAME }))

    await user.type(search(), 'wild')
    await user.keyboard('{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Wild Words')
  })

  it('stays above the favourites, and above how to star one when Favorites only finds none', async () => {
    favoritesDesktop(['Komika Text'])
    const user = userEvent.setup()
    renderPicker({ noneLabel: SAME }, { booharu: true })
    await user.click(screen.getByRole('button', { name: 'Font' }))
    await screen.findByText('Favorites')

    expect(listed()).toEqual([
      SAME,
      'Komika Text',
      'Anime Ace',
      'CCMeanwhile',
      'Komika Text',
      'Wild Words',
    ])

    setFavoritesOnly(true)
    await user.type(search(), 'x')
    await user.clear(search())
    expect(listed()).toEqual([SAME, 'Komika Text'])
    expect(screen.getByRole('status')).toHaveTextContent('1 font')

    await user.click(screen.getByRole('button', { name: 'Remove Komika Text from favorites' }))
    const listbox = await screen.findByRole('listbox')
    expect(await within(listbox).findByText('Star fonts to add them here')).toBeInTheDocument()
    expect(listed()).toEqual([SAME])
    setFavoritesOnly(false)
  })
})
