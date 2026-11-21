'use client'

import { queryOptions, useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'

import { call } from '@/lib/booharu/call'
import { queueBooharuWrite } from '@/lib/booharu/preferences'
import { queryClient } from '@/lib/queries'
import { commands, type FontFamily } from '@koharu/ipc/protocol'

/** What Booharu adds to Koharu's font picker, each optional, so a picker given none is Koharu's;
 * useBooharuFontPicker gives the local names, and a mount its own leading row. Every picker shows
 * the favourites, Booharu's (useBooharuFavoriteFonts). */
export type BooharuFontProps = {
  /** The names each family goes by in other languages, by the name it's listed by, such as
   * 游ゴシック for Yu Gothic: a search finds the family by any of them, and its row shows the
   * first, muted, after its name. */
  localNames?: Readonly<Record<string, readonly string[]>>
  /** The label of a row before the families that stands for no family of the picker's own, such
   * as a lettering category's "Same as Normal Dialogue": the arrows and Enter reach it as they
   * reach a family, picking it gives onChange an empty name, and it shows as chosen, and on the
   * picker's button, while the value is empty. A search leaves it out, as it names no family. */
  noneLabel?: string
}

/** An item of Koharu's font list, as Booharu adds to the families: a favourite's copy pinned above
 * them (`pinned`), or the leading row (`none`, its label), which stands for no family and is named
 * '' so Koharu's filter never finds it. */
export type FontItem = FontFamily & { pinned?: true; none?: string }

/** Koharu's font list's items for the families it lists, `listed`: given `noneLabel`, the leading
 * row first, then a copy of each family among them that `favorites`, normalized names, holds,
 * pinned under Favorites, then every family under All fonts. Koharu's filter finds a pinned copy as
 * it finds its family. */
export function fontItems(
  listed: readonly FontFamily[],
  favorites: readonly string[],
  noneLabel?: string,
): FontItem[] {
  const starred = new Set(favorites)
  const pinned = listed
    .filter((family) => starred.has(fontKey(family.name)))
    .map((family): FontItem => ({ ...family, pinned: true }))
  const leading: FontItem[] =
    noneLabel === undefined ? [] : [{ name: '', faces: [], none: noneLabel }]
  return [...leading, ...pinned, ...listed]
}

/** The leading row among `items` while the picker's `value` is empty, which it then stands for. */
export function leadingItem(items: readonly FontItem[], value: string): FontItem | undefined {
  return !value && items[0]?.none !== undefined ? items[0] : undefined
}

/** The headings over the font list's matches, `items`, by the index of the match each is over:
 * Favorites over the first pinned favourite and All fonts over the first family after them; none
 * while no favourite is among them, as the list is then Koharu's. */
export function fontHeadings(items: readonly FontItem[]): Map<number, 'favorites' | 'allFonts'> {
  const headings = new Map<number, 'favorites' | 'allFonts'>()
  const first = items.findIndex((item) => item.pinned)
  if (first < 0) return headings
  headings.set(first, 'favorites')
  const all = items.findIndex((item, index) => index > first && !item.pinned)
  if (all >= 0) headings.set(all, 'allFonts')
  return headings
}

/** The key of the font list's row for `item`, which Koharu keys `key`, by its family's name: a
 * pinned favourite's and the leading row's start with a space, which no family's key can. */
export function fontItemKey(item: FontItem | undefined, key: string): string {
  if (item?.pinned) return ` pinned ${key}`
  return item?.none === undefined ? key : ' none'
}

/** How many families `items` holds, the pinned favourites' copies and the leading row left out. */
export function familyCount(items: readonly FontItem[]): number {
  return items.filter((item) => !item.pinned && item.none === undefined).length
}

/** A family's name as the font picker compares names, as Koharu's FontPicker normalizes them. */
function fontKey(family: string): string {
  return family.normalize('NFKC').trim().replace(/\s+/g, ' ').toLowerCase()
}

/** `[booharu-fonts]`'s favourites as the desktop answers them, which every font picker shares. A
 * failed read shows as every failed read does. */
const favoritesQuery = queryOptions({
  queryKey: ['booharu', 'font-favorites'] as const,
  queryFn: () => commands.booharuGetFontFavorites(),
})

/** How many stars are on their way to the desktop. */
let starring = 0

/** Stars `family`, or unstars it when the favourites hold it, in every picker at once and then in
 * the settings file, through the desktop, which answers the favourites as it saved them. A failure
 * shows as every failed command's does, and the star goes back. */
async function toggleFontFavorite(family: string): Promise<void> {
  // The star goes over the favourites the file holds, and a read still on its way, which wouldn't
  // have it, is dropped; a read that fails has shown its error.
  const saved = await queryClient.ensureQueryData(favoritesQuery).catch(() => undefined)
  if (saved === undefined) return
  await queryClient.cancelQueries({ queryKey: favoritesQuery.queryKey })
  // The favourites as they show, a star still on its way among them; the file keeps a favourite by
  // the name it was starred under, which unstarring it must give.
  const shown = queryClient.getQueryData(favoritesQuery.queryKey) ?? saved ?? []
  const kept = shown.find((star) => fontKey(star) === fontKey(family))
  const favorite = kept === undefined
  const name = kept ?? family
  const starred = (stars: string[] | null | undefined, on: boolean) => [
    ...(stars ?? []).filter((star) => star !== name),
    ...(on ? [name] : []),
  ]
  starring += 1
  queryClient.setQueryData(favoritesQuery.queryKey, (stars) => starred(stars, favorite))
  try {
    // In turn with the settings saves, which write `[booharu-fonts]` too, as they read it.
    const answer = await queueBooharuWrite(() =>
      call(commands.booharuSetFontFavorite, name, favorite),
    )
    // A later star still on its way shows already, and this answer, given before it, lacks it.
    if (starring === 1) queryClient.setQueryData(favoritesQuery.queryKey, answer)
  } catch {
    queryClient.setQueryData(favoritesQuery.queryKey, (stars) => starred(stars, !favorite))
  } finally {
    starring -= 1
  }
}

const noFavorites: string[] = []

/** The favourites Koharu's font picker shows and stars, Booharu's, kept in `[booharu-fonts]`, which
 * the agent reads and stars too, in place of the ones Koharu keeps in the browser profile:
 * `favorites` by their names as Koharu's picker compares them, and `toggle`, which stars a family
 * by the name it's listed by, or unstars it. */
export function useBooharuFavoriteFonts(): {
  favorites: string[]
  toggle: (family: string) => void
} {
  const saved = useQuery(favoritesQuery).data
  const favorites = useMemo(() => (saved ? saved.map(fontKey) : noFavorites), [saved])
  return { favorites, toggle: starFamily }
}

function starFamily(family: string): void {
  void toggleFontFavorite(family)
}

/** The names font families go by in other languages, as the desktop answers them: read again each
 * time a view with pickers opens, as a bundled family downloaded since has names the last read
 * lacked. The pickers work without them until they come, and with the last read's while the next
 * is on its way; a failed read shows nothing, as a failed font preview shows nothing. */
const localNamesQuery = queryOptions({
  queryKey: ['booharu', 'font-local-names'] as const,
  queryFn: () => commands.booharuGetFontLocalNames(),
  gcTime: Number.POSITIVE_INFINITY,
  refetchOnMount: 'always',
  meta: { booharuQuiet: true },
})

/** Booharu's props for Koharu's font picker, which a mount spreads into it: the families' names in
 * other languages. It reads the favourites again too, which every picker shows, as the agent's
 * settings tool may have changed them since the view last opened. */
export function useBooharuFontPicker(): BooharuFontProps {
  useQuery({ ...favoritesQuery, refetchOnMount: 'always' })
  const localNames = useQuery(localNamesQuery).data
  return {
    localNames: localNames ?? undefined, // A desktop answering null, as a stub may, has none.
  }
}
