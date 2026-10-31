import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { FontPicker } from '@/components/controls/FontPicker'
import { queryClient } from '@/lib/queries'
import { commands, type FontFamily } from '@koharu/bridge/protocol'

const families: FontFamily[] = ['Anime Ace', 'CCMeanwhile', 'Komika Text', 'Wild Words'].map(
  (name) => ({
    name,
    local_names: [],
    sources: ['system'],
    metadata: {
      primary_script: 'latn',
      scripts: ['latn'],
      languages: ['en'],
      category: null,
      classifications: [],
      use_cases: [],
    },
    faces: [
      {
        postscript_name: name.replaceAll(' ', ''),
        weight: 400,
        weight_range: null,
        style: 'normal',
      },
    ],
  }),
)

function picker(onChange = vi.fn()) {
  return render(
    <QueryClientProvider client={queryClient}>
      <FontPicker value='Komika Text' families={families} onChange={onChange} ariaLabel='Font' />
    </QueryClientProvider>,
  )
}

describe('font favorites', () => {
  let saved: string[]

  beforeEach(() => {
    saved = []
    vi.spyOn(commands, 'getFontFavorites').mockImplementation(async () => [...saved])
    vi.spyOn(commands, 'setFontFavorite').mockImplementation(async (family, favorite) => {
      saved = favorite ? [...new Set([...saved, family])] : saved.filter((name) => name !== family)
      return [...saved]
    })
    vi.spyOn(commands, 'getFontPreview').mockResolvedValue([])
  })

  it('finds a family by its localized name and shows that name beside the family name', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    const local: FontFamily = {
      ...families[0]!,
      name: 'Source Han Sans',
      local_names: ['源ノ角ゴシック'],
    }
    render(
      <QueryClientProvider client={queryClient}>
        <FontPicker value='' families={[local, ...families]} onChange={onChange} ariaLabel='Font' />
      </QueryClientProvider>,
    )
    await user.click(screen.getByRole('button', { name: 'Font' }))
    await user.type(screen.getByRole('combobox', { name: 'Search fonts' }), '源ノ角')
    const option = await screen.findByRole('option', { name: /Source Han Sans/ })
    expect(option).toHaveTextContent('源ノ角ゴシック')
    expect(screen.queryByRole('option', { name: /Komika Text/ })).not.toBeInTheDocument()
    await user.click(option)
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Source Han Sans')
  })

  it('keeps the empty choice first and keyboard accessible while filtering', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(
      <QueryClientProvider client={queryClient}>
        <FontPicker
          value='Komika Text'
          families={families}
          onChange={onChange}
          ariaLabel='Font'
          emptyLabel='Same as default'
        />
      </QueryClientProvider>,
    )
    await user.click(screen.getByRole('button', { name: 'Font' }))
    expect(screen.getAllByRole('option')[0]).toHaveAccessibleName('Same as default')
    await user.type(screen.getByRole('combobox', { name: 'Search fonts' }), 'missing family')
    await user.keyboard('{Enter}')
    expect(onChange).toHaveBeenCalledExactlyOnceWith('')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
  })

  it('stars without selecting or closing, and restores favorites when the selector is reopened', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    const view = picker(onChange)
    await user.click(screen.getByRole('button', { name: 'Font' }))
    const star = await screen.findByRole('button', { name: 'Add Wild Words to favorites' })
    await waitFor(() => expect(star).toBeEnabled())
    await user.click(star)
    await screen.findByText('Favorites')

    expect(onChange).not.toHaveBeenCalled()
    expect(screen.getAllByRole('option')[0]).toHaveAccessibleName(/Wild Words/)
    expect(
      screen.getAllByRole('button', { name: 'Remove Wild Words from favorites' })[0],
    ).toHaveAttribute('aria-pressed', 'true')

    view.unmount()
    queryClient.clear()
    picker(onChange)
    await user.click(screen.getByRole('button', { name: 'Font' }))
    await screen.findByText('Favorites')
    expect(screen.getAllByRole('option')[0]).toHaveAccessibleName(/Wild Words/)
    await user.click(screen.getAllByRole('option', { name: /Wild Words/ })[0])
    expect(onChange).toHaveBeenCalledExactlyOnceWith('Wild Words')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
  })

  it('filters both sections and removes a favorite using the keyboard', async () => {
    saved = ['Wild Words']
    const user = userEvent.setup()
    const onChange = vi.fn()
    picker(onChange)
    await user.click(screen.getByRole('button', { name: 'Font' }))
    await screen.findByText('Favorites')
    await user.type(screen.getByRole('combobox', { name: 'Search fonts' }), 'Wild')
    expect(screen.getAllByRole('option')).toHaveLength(2)
    expect(screen.queryByRole('option', { name: /Anime Ace/ })).not.toBeInTheDocument()

    const star = screen.getAllByRole('button', { name: 'Remove Wild Words from favorites' })[0]
    star.focus()
    await user.keyboard('{Enter}')
    await waitFor(() => expect(screen.queryByText('Favorites')).not.toBeInTheDocument())
    expect(screen.getByRole('button', { name: 'Add Wild Words to favorites' })).toHaveAttribute(
      'aria-pressed',
      'false',
    )
    expect(onChange).not.toHaveBeenCalled()
    expect(screen.getByRole('listbox')).toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: 'Search fonts' })).toHaveFocus()
  })
})
