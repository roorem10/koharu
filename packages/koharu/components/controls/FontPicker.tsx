'use client'

import {
  defaultRangeExtractor,
  observeElementRect,
  useVirtualizer,
  type Virtualizer,
} from '@tanstack/react-virtual'
import { Check, ChevronDown, Search, X } from 'lucide-react'
import {
  useEffect,
  useId,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type Ref,
  type RefObject,
} from 'react'
import { useTranslation } from 'react-i18next'

// Booharu: the favourites.
import {
  FavoritesOnlySwitch,
  FavoriteStars,
  FontHeading,
} from '@/components/booharu/fonts/FontFavorites'
import { activeRow, fontRows, type BooharuFontProps, type FontRow } from '@/lib/booharu/fonts'
import { useFontPreview } from '@/lib/queries'
import type { FontFamily } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { Empty, EmptyTitle } from '@koharu/ui/components/empty'
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from '@koharu/ui/components/input-group'
import { Popover, PopoverContent, PopoverTrigger } from '@koharu/ui/components/popover'
import { ScrollArea } from '@koharu/ui/components/scroll-area'
import { Separator } from '@koharu/ui/components/separator'
import { cn } from '@koharu/ui/lib/utils'

const rowHeight = 38
const listHeight = 248
/** Booharu: a section's heading, Favorites or All fonts. */
const headingHeight = 24
const height = (row: FontRow | undefined) => (row && 'heading' in row ? headingHeight : rowHeight)

/** Booharu: the list's scrolling, which brings the family the arrows move to into view. */
type Scroller = Virtualizer<HTMLDivElement, Element>

export function FontPicker({
  value,
  families,
  disabled,
  size = 'default',
  ariaLabel,
  placeholder,
  onChange,
  ...booharu // Booharu: favourites and local names, as lib/booharu/fonts.ts's hook gives them.
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
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const input = useRef<HTMLInputElement>(null)
  // Booharu: the family the arrows move to and Enter picks, by its row's key, as ModelPicker's
  // arrows and Enter go through its models; null for the first. The pointer moves it too.
  const id = useId()
  const [active, setActive] = useState<string | null>(null)
  const scroller = useRef<Scroller>(null)
  const orderedFamilies = useMemo(
    () =>
      [...families].sort((left, right) =>
        left.name.localeCompare(right.name, undefined, { numeric: true, sensitivity: 'base' }),
      ),
    [families],
  )
  const results = useMemo(() => {
    const normalized = normalizeFontName(query)
    return orderedFamilies.filter(
      (family) =>
        !normalized ||
        [
          family.name,
          ...family.faces.map((face) => face.post_script_name),
          ...(booharu.localNames?.[family.name] ?? []), // Booharu: its names in other languages.
        ].some((name) => normalizeFontName(name).includes(normalized)),
    )
  }, [orderedFamilies, query, booharu.localNames])
  const selectedFamily = useMemo(
    () =>
      orderedFamilies.find((family) => normalizeFontName(family.name) === normalizeFontName(value)),
    [orderedFamilies, value],
  )
  // Booharu: the matches, the favourites among them pinned above them, or alone, each with its
  // first name in another language.
  const rows = useMemo(
    () => fontRows(results, booharu.favorites, booharu.favoritesOnly, booharu.localNames),
    [results, booharu.favorites, booharu.favoritesOnly, booharu.localNames],
  )
  // Booharu: the family the arrows are on, which they move from, skipping the headings and
  // wrapping around as ModelPicker's do; on a favourite whose pinned row goes, as when it's
  // unstarred, they stay on its row under All fonts.
  const options = rows.flatMap((row, index) => ('family' in row ? [index] : []))
  const found = activeRow(rows, active)
  const shown = found >= 0 ? found : (options[0] ?? -1)
  const shownRow = rows.at(shown)
  const shownFamily = shownRow && 'family' in shownRow ? shownRow.family : undefined
  const move = (by: number) => {
    if (!options.length) return
    const next = options[(options.indexOf(shown) + by + options.length) % options.length]
    setActive(rows[next].key)
    // The first family brings the list's top into view, with the heading above it.
    scroller.current?.scrollToIndex(next === options[0] ? 0 : next, { align: 'auto' })
  }
  const pick = (family: string) => {
    onChange(family)
    setOpen(false)
  }

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
        if (!next) setQuery('')
        // Booharu: the arrows start from the font in use, pinned or not.
        const current = rows.find(
          (row) =>
            'family' in row && normalizeFontName(row.family.name) === normalizeFontName(value),
        )
        if (next) setActive(current?.key ?? null)
      }}
    >
      <PopoverTrigger
        render={
          <Button
            variant='outline'
            size={size === 'sm' ? 'xs' : 'default'}
            className='w-full min-w-0 justify-between font-normal'
          />
        }
        disabled={disabled}
        aria-label={ariaLabel}
        data-testid='type-font-picker'
        className={cn(
          'w-full min-w-0 justify-between font-normal',
          size === 'sm' ? 'h-6 gap-1 px-1.5 text-[11px]' : 'h-8 gap-2 px-2.5 text-[12px]',
        )}
      >
        {selectedFamily ? (
          <FontPreviewLabel
            family={selectedFamily}
            className='min-w-0 flex-1 py-1 [&>img]:max-h-3.5'
          />
        ) : (
          <span className='truncate'>{value || placeholder || t('fontPicker.choose')}</span>
        )}
        <ChevronDown className='size-3.5 shrink-0 text-muted-foreground' />
      </PopoverTrigger>
      <PopoverContent
        align='start'
        className='w-(--anchor-width) min-w-44 gap-0 overflow-hidden rounded-lg p-0'
        initialFocus={input}
      >
        <div className='border-b p-1'>
          <InputGroup className='h-6 border-0 bg-muted/50 shadow-none focus-within:bg-background has-[>[data-align=inline-start]]:[&>input]:pl-0'>
            <InputGroupAddon className='pr-1 pl-1.5'>
              <Search className='size-3' />
            </InputGroupAddon>
            <InputGroupInput
              ref={input}
              value={query}
              aria-label={t('fontPicker.search')}
              placeholder={t('fontPicker.search')}
              className='h-6 px-0 text-[11px]'
              onChange={(event) => {
                setQuery(event.currentTarget.value)
                setActive(null) // Booharu: the arrows start again from the first match.
              }}
              // Booharu: a combobox whose arrows move through the families and whose Enter picks
              // one, but not while an IME is composing, whose candidates they choose.
              role='combobox'
              aria-expanded={open}
              aria-autocomplete='list'
              aria-controls={`${id}-fonts`}
              aria-activedescendant={shownFamily ? `${id}-${shown}` : undefined}
              onKeyDown={(event) => {
                if (event.nativeEvent.isComposing) return
                if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
                  event.preventDefault()
                  move(event.key === 'ArrowDown' ? 1 : -1)
                } else if (event.key === 'Enter' && shownFamily) {
                  event.preventDefault()
                  pick(shownFamily.name)
                }
              }}
            />
            {query && (
              <InputGroupAddon align='inline-end' className='gap-0.5 pr-0.5'>
                <InputGroupButton aria-label={t('common.clearSearch')} onClick={() => setQuery('')}>
                  <X />
                </InputGroupButton>
              </InputGroupAddon>
            )}
            {/* Booharu: the switch that shows only the favourites. */}
            <FavoritesOnlySwitch booharu={booharu} search={input} />
          </InputGroup>
        </div>
        {/* Booharu: the count, and the family the arrows are on in its own face, read out as they
            change. */}
        <div
          role='status'
          className='flex h-6 items-center justify-between gap-2 px-2 text-[9px] text-muted-foreground'
        >
          <span className='shrink-0'>
            {t('fontPicker.fontCount', {
              count: booharu.favoritesOnly ? options.length : results.length,
            })}
          </span>
          {shownFamily && (
            <FontPreviewLabel
              family={shownFamily}
              className='min-w-0 justify-end [&>img]:max-h-3.5 [&>img]:object-right'
            />
          )}
        </div>
        <Separator />
        {open && (
          <FontList
            key={`${booharu.favoritesOnly ?? false}:${query}`}
            id={id}
            rows={rows}
            value={value}
            active={shown}
            scroller={scroller}
            onActive={setActive}
            onSelect={pick}
            booharu={{ ...booharu, search: input }}
          />
        )}
      </PopoverContent>
    </Popover>
  )
}

function FontList({
  id,
  rows,
  value,
  active,
  scroller,
  onActive,
  onSelect,
  booharu,
}: {
  /** Booharu: names the list and its options, for the search line's active descendant. */
  id: string
  /** Booharu: the families, under their sections' headings when there are favourites. */
  rows: FontRow[]
  value: string
  /** Booharu: the family the arrows are on, which the list opens on and marks and `scroller`
   * scrolls to; `onActive` moves it to the family the pointer or the focus is on, by its row's
   * key. */
  active: number
  scroller: Ref<Scroller>
  onActive: (key: string) => void
  onSelect: (family: string) => void
  /** Booharu: the favourites, and the search line, which a star gives the focus back to. */
  booharu: BooharuFontProps & { search: RefObject<HTMLInputElement | null> }
}) {
  const { t } = useTranslation()
  const list = useRef<HTMLDivElement>(null)
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => list.current,
    getItemKey: (index) => rows[index]?.key ?? index,
    estimateSize: (index) => height(rows[index]),
    overscan: 6,
    initialOffset: Math.max(
      0,
      rows.slice(0, Math.max(active, 0)).reduce((top, row) => top + height(row), 0) -
        Math.floor((listHeight / rowHeight - 1) / 2) * rowHeight,
    ),
    initialRect: { width: 240, height: listHeight },
    observeElementRect: (instance, callback) =>
      observeElementRect(instance, (rect) =>
        callback({ width: rect.width || 240, height: rect.height || listHeight }),
      ),
    // Booharu: the family the arrows are on stays in the list out of view too, so the search
    // line's active descendant is always there.
    rangeExtractor: (range) => {
      const shown = defaultRangeExtractor(range)
      return active < 0 || active >= range.count || shown.includes(active)
        ? shown
        : [...shown, active].sort((left, right) => left - right)
    },
  })
  useImperativeHandle(scroller, () => virtualizer, [virtualizer])

  return (
    <ScrollArea
      viewportRef={list}
      role='listbox'
      id={`${id}-fonts`}
      aria-label={t('fontPicker.fonts')}
      className='relative'
      viewportClassName='overflow-x-hidden'
      style={{
        height:
          rows.length === 0
            ? rowHeight * 2
            : Math.min(listHeight, Math.max(rowHeight, virtualizer.getTotalSize())),
      }}
    >
      <div className='relative' style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const row = rows[virtualRow.index]
          if (!row) return null
          // Booharu: a section's heading, which the arrows skip.
          if ('heading' in row) {
            return <FontHeading key={virtualRow.key} heading={row.heading} top={virtualRow.start} />
          }
          const family = row.family
          const selected = normalizeFontName(family.name) === normalizeFontName(value)
          return (
            <button
              key={virtualRow.key}
              type='button'
              role='option'
              aria-label={family.name}
              aria-selected={selected}
              // Booharu: the family the arrows are on is marked, and the one in use checked, as
              // ModelPicker marks and checks its models.
              id={`${id}-${virtualRow.index}`}
              data-active={virtualRow.index === active}
              className={cn(
                'absolute inset-x-0 top-0 flex h-9.5 w-full items-center gap-2 px-2 text-left focus-visible:outline-none data-[active=true]:bg-accent data-[active=true]:text-accent-foreground',
                booharu.onToggleFavorite && 'pr-9', // Booharu: room for the star.
              )}
              style={{ transform: `translateY(${virtualRow.start}px)` }}
              // Booharu: only the pointer's own moves move the mark, as in ModelPicker: rows
              // passing under a still pointer as the list scrolls move nothing.
              onPointerMove={(event) => {
                if (event.movementX || event.movementY) onActive(row.key)
              }}
              onFocus={() => onActive(row.key)}
              onClick={() => onSelect(family.name)}
            >
              <div className='flex min-w-0 flex-1 flex-col justify-center gap-px'>
                <FontPreviewLabel family={family} className='h-4.5 w-full min-w-0' />
                <span className='min-w-0 truncate text-[8px] leading-none text-muted-foreground'>
                  {family.name}
                  {/* Booharu: its first name in another language, muted, as 0.0.4 showed it. */}
                  {row.localName && <span className='ml-1 opacity-70'>{row.localName}</span>}
                </span>
              </div>
              {selected && <Check className='size-3.5 shrink-0' />}
            </button>
          )
        })}
        {/* Booharu: each family's star, over its row's end. */}
        <FavoriteStars
          rows={rows}
          items={virtualizer.getVirtualItems()}
          booharu={booharu}
          onActive={onActive}
        />
      </div>
      {rows.length === 0 && (
        <Empty role='status' className='absolute inset-1 rounded-md p-2'>
          <EmptyTitle className='text-[11px] text-muted-foreground'>
            {/* Booharu: how to star a font, when only the favourites show and there are none. */}
            {booharu.favoritesOnly && !booharu.favorites?.length
              ? t('booharu.fonts.noFavorites')
              : t('fontPicker.noResults')}
          </EmptyTitle>
        </Empty>
      )}
    </ScrollArea>
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
