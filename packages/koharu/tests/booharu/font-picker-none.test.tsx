import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import {
  favoritesDesktop,
  highlighted,
  listed,
  openPicker,
  renderPicker,
  search,
  trigger,
} from './fonts'

const SAME = 'Same as Normal Dialogue'

describe('Booharu font picker leading row', () => {
  it('shows an empty value as the leading row, chosen, and reaches it with the arrows', async () => {
    const user = userEvent.setup()
    const { onChange } = renderPicker({ noneLabel: SAME })

    // On the button as a family's name is, not muted as a placeholder is.
    expect(trigger()).toHaveTextContent(SAME)
    expect(within(trigger()).getByText(SAME)).not.toHaveClass('text-muted-foreground')
    await openPicker(user)

    // First, before the families, chosen, and where the arrows start.
    expect(listed()).toEqual([SAME, 'Anime Ace', 'CCMeanwhile', 'Komika Text', 'Wild Words'])
    const none = screen.getByRole('option', { name: SAME })
    expect(none).toHaveAttribute('aria-selected', 'true')
    expect(highlighted()).toBe(none)
    // The families count, the leading row not among them.
    expect(screen.getByText('4 fonts')).toBeInTheDocument()

    await user.keyboard('{ArrowDown}')
    expect(highlighted()).toHaveAccessibleName('Anime Ace')
    await user.keyboard('{ArrowUp}')
    expect(highlighted()).toBe(screen.getByRole('option', { name: SAME }))

    await user.keyboard('{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('')
    await waitFor(() => expect(screen.queryByRole('listbox')).not.toBeInTheDocument())
  })

  it('starts from the family in use, and picking the leading row gives no family', async () => {
    const user = userEvent.setup()
    const { onChange } = renderPicker({ value: 'Komika Text', noneLabel: SAME })
    await openPicker(user)

    expect(highlighted()).toHaveAccessibleName('Komika Text')
    expect(screen.getByRole('option', { name: SAME })).toHaveAttribute('aria-selected', 'false')
    expect(screen.getByRole('option', { name: 'Komika Text' })).toHaveAttribute(
      'aria-selected',
      'true',
    )

    await user.click(screen.getByRole('option', { name: SAME }))
    expect(onChange).toHaveBeenCalledExactlyOnceWith('')
  })

  it('leaves a search to the families', async () => {
    const user = userEvent.setup()
    const { onChange } = renderPicker({ noneLabel: SAME })
    await openPicker(user)

    await user.type(search(), 'w')
    expect(listed()).toEqual(['CCMeanwhile', 'Wild Words'])
    await user.keyboard('{ArrowDown}')
    expect(highlighted()).toHaveAccessibleName('CCMeanwhile')

    // Cleared, the search shows it again.
    await user.clear(search())
    expect(listed()[0]).toBe(SAME)

    await user.type(search(), 'wild')
    await user.keyboard('{ArrowDown}{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Wild Words')
  })

  // Show favorite fonts stays on for the next test, so this one turns it off again at its end.
  it('stays above the favourites, and with Show favorite fonts above them alone', async () => {
    favoritesDesktop(['Komika Text'])
    const user = userEvent.setup()
    renderPicker({ noneLabel: SAME }, { booharu: true })
    await openPicker(user)
    await screen.findByText('Favorites')

    expect(listed()).toEqual([
      SAME,
      'Komika Text',
      'Anime Ace',
      'CCMeanwhile',
      'Komika Text',
      'Wild Words',
    ])

    const only = screen.getByRole('button', { name: 'Show favorite fonts' })
    await user.click(only)
    expect(listed()).toEqual([SAME, 'Komika Text'])
    expect(screen.getByText('1 font')).toBeInTheDocument()

    // With none left, the leading row still offers no family.
    await user.click(screen.getByRole('button', { name: 'Remove Komika Text from favorites' }))
    await waitFor(() => expect(listed()).toEqual([SAME]))

    await user.click(only)
    expect(only).toHaveAttribute('aria-pressed', 'false')
  })
})
