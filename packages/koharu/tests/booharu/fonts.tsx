import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import type { UserEvent } from '@testing-library/user-event'
import type { ComponentProps } from 'react'
import { vi } from 'vitest'

import { FontPicker } from '@/components/controls/FontPicker'
import { useBooharuFontPicker } from '@/lib/booharu/fonts'
import { queryClient } from '@/lib/queries'
import { commands, type FontFamily } from '@koharu/ipc/protocol'

/** Installed families by name, each with one regular face, its PostScript name without spaces. */
export function fontFamilies(...names: string[]): FontFamily[] {
  return names.map((name) => ({
    name,
    faces: [
      {
        post_script_name: name.replaceAll(' ', ''),
        weight: 400,
        weight_range: null,
        italic: false,
      },
    ],
  }))
}

/** Four comic fonts, in name order. */
export const comicFonts = fontFamilies('Anime Ace', 'CCMeanwhile', 'Komika Text', 'Wild Words')

type PickerProps = ComponentProps<typeof FontPicker>

/** Koharu's font picker as Booharu mounts it, with useBooharuFontPicker's props. */
function BooharuFontPicker(props: PickerProps) {
  return <FontPicker {...props} {...useBooharuFontPicker()} />
}

/** Koharu's font picker, named Font, choosing among the comic fonts unless `props` say otherwise,
 * as Koharu mounts it or, with `booharu`, as Booharu does; every family's preview is a blank
 * image. Every picker shows Booharu's favourites: none, unless favoritesDesktop answers some. */
export function renderPicker(props: Partial<PickerProps> = {}, { booharu = false } = {}) {
  vi.spyOn(commands, 'getFontPreview').mockResolvedValue(new ArrayBuffer(0))
  if (!vi.isMockFunction(commands.booharuGetFontFavorites)) favoritesDesktop()
  const onChange = vi.fn()
  const Picker = booharu ? BooharuFontPicker : FontPicker
  render(
    <QueryClientProvider client={queryClient}>
      <Picker value='' families={comicFonts} ariaLabel='Font' onChange={onChange} {...props} />
    </QueryClientProvider>,
  )
  return { onChange }
}

/** The desktop's `[booharu-fonts]` favourites, which it answers sorted and changes one at a time;
 * `set` is the command that stars or unstars one. */
export function favoritesDesktop(saved: string[] = []) {
  const file = { favorites: [...saved] }
  vi.spyOn(commands, 'booharuGetFontFavorites').mockImplementation(async () =>
    [...file.favorites].sort(),
  )
  const set = vi
    .spyOn(commands, 'booharuSetFontFavorite')
    .mockImplementation(async (family, favorite) => {
      file.favorites = file.favorites.filter((name) => name !== family)
      if (favorite) file.favorites.push(family)
      return [...file.favorites].sort()
    })
  return { file, set }
}

/** The picker's button, named Font, which opens its list. */
export function trigger(name = 'Font'): HTMLElement {
  return screen.getByRole('combobox', { name })
}

/** Opens the picker named `name` with the pointer, as `user`, once its list shows. */
export async function openPicker(user: UserEvent, name = 'Font'): Promise<void> {
  await user.click(trigger(name))
  await screen.findByRole('listbox')
}

/** The search line, a combobox over the fonts. */
export function search(): HTMLElement {
  return screen.getByRole('combobox', { name: 'Search fonts' })
}

/** The font the arrows are on: the option the search line names as its active descendant. */
export function highlighted(): HTMLElement | null {
  const id = search().getAttribute('aria-activedescendant')
  return id ? document.getElementById(id) : null
}

/** The fonts the list shows, by name, in order, a pinned favourite and its row under All fonts
 * each. */
export function listed(): string[] {
  return screen.queryAllByRole('option').map((option) => option.getAttribute('aria-label') ?? '')
}
