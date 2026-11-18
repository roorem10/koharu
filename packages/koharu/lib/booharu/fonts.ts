'use client'

import { queryOptions, useQuery } from '@tanstack/react-query'
import { create } from 'zustand'

import { call } from '@/lib/backend'
import { queryClient } from '@/lib/queries'
import { commands, type FontFamily } from '@koharu/ipc/protocol'

/** What Booharu adds to Koharu's font picker, each optional, so a picker given none is Koharu's;
 * useBooharuFontPicker gives them all. */
export type BooharuFontProps = {
  /** The favourite families, by name: those among the matches are pinned under Favorites, above
   * All fonts. */
  favorites?: readonly string[]
  /** Stars `family`, or with `favorite` false unstars it: each family's row gets a star for it. */
  onToggleFavorite?: (family: string, favorite: boolean) => void
  /** Whether only the favourites show. */
  favoritesOnly?: boolean
  /** Turns Favorites only on or off: the search line gets a switch for it. */
  onFavoritesOnlyChange?: (favoritesOnly: boolean) => void
}

/** A row of the font list: a family, starred when it's a favourite, or a section's heading, which
 * the arrows skip. */
export type FontRow =
  | { key: string; family: FontFamily; starred: boolean }
  | { key: string; heading: 'favorites' | 'allFonts' }

/** The font list's rows for `results`, the families matching the search, in name order: the
 * favourites among them pinned under Favorites, then every match under All fonts, or with
 * `favoritesOnly` the favourites alone; with no favourite among them, the matches alone, as Koharu
 * lists them. A pinned family's key is its own under All fonts with `favorite:` before it. */
export function fontRows(
  results: FontFamily[],
  favorites: readonly string[] = [],
  favoritesOnly = false,
): FontRow[] {
  const starred = new Set(favorites.map(fontKey))
  const all = results.map((family) => {
    const key = fontKey(family.name)
    return { key, family, starred: starred.has(key) }
  })
  const pinned: FontRow[] = all
    .filter((row) => row.starred)
    .map((row) => ({ ...row, key: `favorite:${row.key}` }))
  const favoriteRows: FontRow[] = pinned.length
    ? [{ key: 'heading:favorites', heading: 'favorites' }, ...pinned]
    : []
  if (favoritesOnly) return favoriteRows
  if (!pinned.length) return all
  return [...favoriteRows, { key: 'heading:allFonts', heading: 'allFonts' }, ...all]
}

/** A family's name as the font picker compares names, as Koharu's FontPicker normalizes them. */
function fontKey(family: string): string {
  return family.normalize('NFKC').trim().replace(/\s+/g, ' ').toLowerCase()
}

/** `[booharu-fonts]`'s favourites as the desktop answers them, which every font picker shares:
 * read again each time a view with pickers opens, as the agent's settings tool may have changed
 * them since. */
const favoritesQuery = queryOptions({
  queryKey: ['booharu', 'font-favorites'] as const,
  queryFn: () => call(commands.booharuGetFontFavorites),
  refetchOnMount: 'always',
})

/** How many stars are on their way to the desktop. */
let starring = 0

/** Stars `family`, or with `favorite` false unstars it, in every picker at once and then in the
 * settings file, through the desktop, which answers the favourites as it saved them. A failure
 * shows as every failed command's does, and the star goes back. */
async function toggleFontFavorite(family: string, favorite: boolean): Promise<void> {
  // The star goes over the favourites the file holds, and a read still on its way, which wouldn't
  // have it, is dropped; a read that fails has shown its error.
  const saved = await queryClient.ensureQueryData(favoritesQuery).catch(() => undefined)
  if (saved === undefined) return
  await queryClient.cancelQueries({ queryKey: favoritesQuery.queryKey })
  // The file keeps a favourite by the name it was starred under, which unstarring it must give.
  const name = favorite
    ? family
    : ((saved ?? []).find((star) => fontKey(star) === fontKey(family)) ?? family)
  const starred = (stars: string[] | null | undefined, on: boolean) => [
    ...(stars ?? []).filter((star) => star !== name),
    ...(on ? [name] : []),
  ]
  starring += 1
  queryClient.setQueryData(favoritesQuery.queryKey, (stars) => starred(stars, favorite))
  try {
    const answer = await call(commands.booharuSetFontFavorite, name, favorite)
    // A later star still on its way shows already, and this answer, given before it, lacks it.
    if (starring === 1) queryClient.setQueryData(favoritesQuery.queryKey, answer)
  } catch {
    queryClient.setQueryData(favoritesQuery.queryKey, (stars) => starred(stars, !favorite))
  } finally {
    starring -= 1
  }
}

/** Whether the font pickers show only the favourites: one choice for every picker, kept for the
 * session only. */
const useFavoritesOnly = create<{ favoritesOnly: boolean }>()(() => ({ favoritesOnly: false }))

export function setFavoritesOnly(favoritesOnly: boolean): void {
  useFavoritesOnly.setState({ favoritesOnly })
}

const noFavorites: readonly string[] = []

/** Booharu's props for Koharu's font picker, which a mount spreads into it: the favourites, read
 * once and shared by every picker, with their stars, and the Favorites only switch. */
export function useBooharuFontPicker(): BooharuFontProps {
  const favorites = useQuery(favoritesQuery).data
  const favoritesOnly = useFavoritesOnly((state) => state.favoritesOnly)
  return {
    favorites: favorites ?? noFavorites,
    onToggleFavorite: toggleFontFavorite,
    favoritesOnly,
    onFavoritesOnlyChange: setFavoritesOnly,
  }
}
