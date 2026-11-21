'use client'

import { observeElementRect, useVirtualizer } from '@tanstack/react-virtual'
import { Star } from 'lucide-react'
import { useEffect, useImperativeHandle, useMemo, useRef, useState, type Ref } from 'react'
import { useTranslation } from 'react-i18next'
import { create } from 'zustand'
import { persist } from 'zustand/middleware'

// Booharu: the sections' headings, the leading row, the local names, and the favourites, which the
// settings file keeps.
import { FontHeadings } from '@/components/booharu/fonts/FontFavorites'
import { NoneOption } from '@/components/booharu/fonts/NoneOption'
import { PickerSearch } from '@/components/controls/PickerSearch'
import {
  familyCount,
  fontHeadings,
  fontItemKey,
  fontItems,
  leadingItem,
  useBooharuFavoriteFonts,
  type BooharuFontProps,
  type FontItem,
} from '@/lib/booharu/fonts'
import { useFontPreview } from '@/lib/queries'
import type { FontFamily } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxItem,
  ComboboxList,
  ComboboxTrigger,
  useComboboxFilteredItems,
} from '@koharu/ui/components/combobox'
import { ScrollArea } from '@koharu/ui/components/scroll-area'
import { Separator } from '@koharu/ui/components/separator'
import { Toggle } from '@koharu/ui/components/toggle'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'
import { cn } from '@koharu/ui/lib/utils'

const rowHeight = 38
const listHeight = 248
/** Booharu: a section's heading, Favorites or All fonts, over its first row. */
const headingHeight = 24

/** The families starred as favorites, by normalized name, and whether the picker lists only them,
 * as Photoshop's font menu keeps favorites and its Show Favorite Fonts filter. Every picker shares
 * them, and the interface keeps them in its browser profile, as it does the editor's panel widths. */
const useFavoriteFonts = create<{
  favorites: string[]
  only: boolean
  toggle: (family: string) => void
  showOnly: (only: boolean) => void
}>()(
  persist(
    (set) => ({
      favorites: [],
      only: false,
      toggle: (family) =>
        set(({ favorites }) => {
          const name = normalizeFontName(family)
          return {
            favorites: favorites.includes(name)
              ? favorites.filter((favorite) => favorite !== name)
              : [...favorites, name],
          }
        }),
      showOnly: (only) => set({ only }),
    }),
    { name: 'favorite-fonts' },
  ),
)

/** Picks a font family, each shown in its own face: a search line that matches a family's name or
 * any of its faces' PostScript names, and a star beside it that lists only the favorites, then the
 * matches in alphabetical order, the keyboard's arrows and Enter moving through them. */
export function FontPicker({
  value,
  families,
  disabled,
  size = 'default',
  ariaLabel,
  placeholder,
  onChange,
  // Booharu: the local names, as lib/booharu/fonts.ts's hook gives them, and a mount's leading row.
  ...booharu
}: {
  value: string
  families: FontFamily[]
  disabled?: boolean
  size?: 'default' | 'sm'
  ariaLabel?: string
  placeholder?: string
  onChange: (family: string) => void
} & BooharuFontProps) {
  const { t } = useTranslation()
  const [query, setQuery] = useState('')
  const list = useRef<{ reveal: (index: number) => void }>(null)
  const orderedFamilies = useMemo(
    () =>
      [...families].sort((left, right) =>
        left.name.localeCompare(right.name, undefined, { numeric: true, sensitivity: 'base' }),
      ),
    [families],
  )
  const selectedFamily = useMemo(
    () =>
      orderedFamilies.find((family) => normalizeFontName(family.name) === normalizeFontName(value)),
    [orderedFamilies, value],
  )
  const { only, showOnly } = useFavoriteFonts() // Booharu: its favourites are Booharu's, below.
  const { favorites } = useBooharuFavoriteFonts() // Booharu: the settings file's.
  const listed = useMemo(
    () =>
      only
        ? orderedFamilies.filter((family) => favorites.includes(normalizeFontName(family.name)))
        : orderedFamilies,
    [only, favorites, orderedFamilies],
  )
  // Booharu: the leading row, then the favourites pinned above every family, unless only they show.
  const items = useMemo(
    () => fontItems(listed, only ? [] : favorites, booharu.noneLabel),
    [listed, only, favorites, booharu.noneLabel],
  )

  return (
    <Combobox<FontFamily>
      virtualized
      items={items} // Booharu: with the leading row and the pinned favourites.
      value={selectedFamily ?? leadingItem(items, value) ?? null} // Booharu: or the leading row.
      inputValue={query}
      disabled={disabled}
      itemToStringLabel={(family) => family.name}
      isItemEqualToValue={(family, other) =>
        normalizeFontName(family.name) === normalizeFontName(other.name)
      }
      filter={(family, text) => {
        const normalized = normalizeFontName(text)
        return [
          family.name,
          ...family.faces.map((face) => face.post_script_name),
          ...(booharu.localNames?.[family.name] ?? []), // Booharu: its names in other languages.
        ].some((name) => normalizeFontName(name).includes(normalized))
      }}
      onInputValueChange={setQuery}
      onOpenChange={(open) => !open && setQuery('')}
      onValueChange={(family) => family && onChange(family.name)}
      onItemHighlighted={(_, { reason, index }) => {
        if (reason === 'keyboard') list.current?.reveal(index)
      }}
    >
      <ComboboxTrigger
        render={
          <Button
            variant='outline'
            size={size === 'sm' ? 'xs' : 'default'}
            className='w-full min-w-0 justify-between font-normal'
          />
        }
        aria-label={ariaLabel}
        data-testid='type-font-picker'
        className={cn(
          'w-full min-w-0 justify-between font-normal [&>svg:last-child]:size-3.5',
          size === 'sm' ? 'h-6 gap-1 px-1.5 text-[11px]' : 'h-8 gap-2 px-2.5 text-[12px]',
        )}
      >
        {selectedFamily ? (
          <FontPreviewLabel
            family={selectedFamily}
            className='min-w-0 flex-1 py-1 [&>img]:max-h-3.5'
          />
        ) : (
          // Booharu: or the leading row's label, which an empty value stands for, not muted.
          <span
            className={cn(
              'truncate',
              !value && booharu.noneLabel === undefined && 'text-muted-foreground',
            )}
          >
            {value || booharu.noneLabel || placeholder || t('fontPicker.choose')}
          </span>
        )}
      </ComboboxTrigger>
      <ComboboxContent className='w-(--anchor-width) min-w-44'>
        <div className='flex items-center gap-1 border-b p-1'>
          <div className='min-w-0 flex-1'>
            <PickerSearch
              query={query}
              label={t('fontPicker.search')}
              onClear={() => setQuery('')}
            />
          </div>
          <Tooltip>
            <TooltipTrigger
              render={
                <Toggle
                  size='sm'
                  aria-label={t('fontPicker.favoritesOnly')}
                  pressed={only}
                  onPressedChange={showOnly}
                  // The search line keeps the keys, so the arrows still move through the list.
                  onMouseDown={(event) => event.preventDefault()}
                  className='size-7 shrink-0 px-0 text-muted-foreground aria-pressed:bg-transparent aria-pressed:text-primary aria-pressed:hover:bg-muted'
                />
              }
            >
              <Star className={cn('size-3.5', only && 'fill-current')} />
            </TooltipTrigger>
            <TooltipContent>{t('fontPicker.favoritesOnly')}</TooltipContent>
          </Tooltip>
        </div>
        <ComboboxEmpty className='py-4'>
          {only && favorites.length === 0 ? t('fontPicker.noFavorites') : t('fontPicker.noResults')}
        </ComboboxEmpty>
        <ComboboxList
          aria-label={t('fontPicker.fonts')}
          className='max-h-none overflow-visible p-0'
        >
          {/* Anew for each query, so a filtered list starts at its top. */}
          <FontList
            key={query}
            selected={value}
            ref={list}
            localNames={booharu.localNames} // Booharu: on the families' rows.
          />
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
  )
}

/** The matches, how many, then each in its own face behind its favorite star, opening on
 * `selected`. Only the rows in view exist, as a system can hold thousands of families; `reveal`
 * scrolls to a match the keyboard reaches. */
function FontList({
  selected,
  ref,
  localNames,
}: {
  selected: string
  ref: Ref<{ reveal: (index: number) => void }>
  /** Booharu: the names each family goes by in other languages, the first shown on its row. */
  localNames?: BooharuFontProps['localNames']
}) {
  const { t } = useTranslation()
  // Booharu: with the leading row and the pinned favourites.
  const families = useComboboxFilteredItems<FontItem>()
  const { favorites, toggle } = useBooharuFavoriteFonts() // Booharu: the settings file's.
  // Booharu: the sections' headings, each over its first row, which is taller by it.
  const headings = fontHeadings(families)
  const selectedIndex = families.findIndex(
    (family) => normalizeFontName(family.name) === normalizeFontName(selected),
  )
  const viewport = useRef<HTMLDivElement>(null)
  const virtualizer = useVirtualizer({
    count: families.length,
    getScrollElement: () => viewport.current,
    getItemKey: (index) =>
      // Booharu: a pinned favourite's key and the leading row's differ from the families'.
      fontItemKey(families[index], normalizeFontName(families[index]?.name ?? String(index))),
    // Booharu: a row under a heading is taller by it.
    estimateSize: (index) => rowHeight + (headings.has(index) ? headingHeight : 0),
    overscan: 6,
    initialOffset: Math.max(
      0,
      selectedIndex * rowHeight -
        Math.floor((listHeight / rowHeight - 1) / 2) * rowHeight +
        // Booharu: and the headings above the chosen family.
        [...headings.keys()].filter((index) => index < selectedIndex).length * headingHeight,
    ),
    initialRect: { width: 240, height: listHeight },
    observeElementRect: (instance, callback) =>
      observeElementRect(instance, (rect) =>
        callback({ width: rect.width || 240, height: rect.height || listHeight }),
      ),
  })

  useImperativeHandle(ref, () => ({
    reveal: (index) => virtualizer.scrollToIndex(index, { align: 'auto' }),
  }))

  if (families.length === 0) return null
  return (
    <>
      <div className='flex h-6 items-center px-2 text-[9px] text-muted-foreground'>
        {/* Booharu: the families, the pinned favourites' copies and the leading row left out. */}
        {t('fontPicker.fontCount', { count: familyCount(families) })}
      </div>
      <Separator />
      <ScrollArea
        viewportRef={viewport}
        className='relative'
        viewportClassName='overflow-x-hidden'
        // Booharu: as tall as its rows, some taller by a heading.
        style={{ height: Math.min(listHeight, virtualizer.getTotalSize()) }}
      >
        <div className='relative' style={{ height: virtualizer.getTotalSize() }}>
          {/* Booharu: the sections' headings over the rows in view. */}
          <FontHeadings rows={virtualizer.getVirtualItems()} headings={headings} />
          {virtualizer.getVirtualItems().map((row) => {
            const family = families[row.index]
            if (!family) return null
            // Booharu: the leading row, which stands for no family.
            if (family.none !== undefined) {
              return (
                <NoneOption
                  key={row.key}
                  index={row.index}
                  item={family}
                  count={families.length}
                  top={row.start}
                />
              )
            }
            const favorite = favorites.includes(normalizeFontName(family.name))
            const localName = localNames?.[family.name]?.[0] // Booharu: its first other name.
            return (
              <ComboboxItem
                key={row.key}
                index={row.index}
                value={family}
                aria-label={family.name}
                aria-setsize={families.length}
                aria-posinset={row.index + 1}
                // The chosen family shows in the accent, as a bar's lists show their chosen item,
                // with no tick. Base UI marks it with a bare `data-selected`, which shadcn's
                // `data-selected:` variant, written for `="true"`, doesn't match. Its right is as
                // wide as the star's strip and gaps on its left, so the face sits between even
                // margins.
                className='group/font absolute inset-x-0 top-0 h-9.5 gap-1 rounded-none pr-5.5 pl-1 data-[selected]:bg-accent data-[selected]:text-accent-foreground data-[selected]:**:text-accent-foreground [&>span]:hidden!'
                // Booharu: below its section's heading, when it has one.
                style={{
                  transform: `translateY(${row.start + (headings.has(row.index) ? headingHeight : 0)}px)`,
                }}
              >
                {/* Starred with the pointer, as in Photoshop's font menu; its click stops here, so
                    it doesn't pick the family, and the item keeps focus on the search line. */}
                <button
                  type='button'
                  tabIndex={-1}
                  aria-pressed={favorite}
                  aria-label={t(favorite ? 'fontPicker.removeFavorite' : 'fontPicker.addFavorite', {
                    name: family.name,
                  })}
                  className={cn(
                    'grid w-3.5 shrink-0 place-items-center self-stretch text-muted-foreground hover:text-foreground',
                    favorite
                      ? 'text-primary'
                      : 'opacity-0 group-hover/font:opacity-100 group-data-highlighted/font:opacity-100',
                  )}
                  onClick={(event) => {
                    event.stopPropagation()
                    toggle(family.name)
                  }}
                >
                  <Star className={cn('size-3', favorite && 'fill-current')} />
                </button>
                <div className='flex min-w-0 flex-1 flex-col justify-center gap-px'>
                  <FontPreviewLabel family={family} className='h-4.5 w-full min-w-0' />
                  <span className='min-w-0 truncate text-[8px] leading-none text-muted-foreground'>
                    {family.name}
                    {/* Booharu: its first name in another language, muted, as 0.0.4 showed it. */}
                    {localName && <span className='ml-1 opacity-70'>{localName}</span>}
                  </span>
                </div>
              </ComboboxItem>
            )
          })}
        </div>
      </ScrollArea>
    </>
  )
}

function FontPreviewLabel({ family, className }: { family: FontFamily; className?: string }) {
  const previewQuery = useFontPreview(family)
  const preview = previewQuery.data
  const [url, setUrl] = useState<string | null>(null)

  useEffect(() => {
    setUrl(null)
    if (!preview) return
    const next = URL.createObjectURL(new Blob([preview.buffer], { type: 'image/webp' }))
    setUrl(next)
    return () => URL.revokeObjectURL(next)
  }, [preview])

  return url ? (
    <span className={cn('flex h-full min-w-0 items-center', className)}>
      <img
        src={url}
        alt={family.name}
        className='max-h-[18px] max-w-full object-contain object-left dark:invert'
      />
    </span>
  ) : (
    <span className={cn('truncate font-sans', className)}>{family.name}</span>
  )
}

function normalizeFontName(value: string): string {
  return value.normalize('NFKC').trim().replace(/\s+/g, ' ').toLowerCase()
}
