'use client'

import { defaultRangeExtractor, observeElementRect, useVirtualizer } from '@tanstack/react-virtual'
import type { TFunction } from 'i18next'
import { ChevronDown, ListFilter, Search, Star, X } from 'lucide-react'
import { useEffect, useId, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useFontFavorites, useFontPreview } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import type { FontFamily, FontSource } from '@koharu/bridge/protocol'
import { Badge } from '@koharu/ui/components/badge'
import { Button } from '@koharu/ui/components/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@koharu/ui/components/dropdown-menu'
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
const headingHeight = 24
const listHeight = 248
const allFonts = '__all_fonts__'

type FontRow =
  | { kind: 'heading'; key: string; label: string }
  | { kind: 'empty'; key: string; label: string }
  | { kind: 'font'; key: string; family: FontFamily }

type Filters = {
  source: '' | FontSource
  script: string
  category: string
  useCase: string
}

const emptyFilters: Filters = { source: '', script: '', category: '', useCase: '' }

export function FontPicker({
  value,
  families,
  disabled,
  size = 'default',
  ariaLabel,
  placeholder,
  emptyLabel,
  onChange,
}: {
  value: string
  families: FontFamily[]
  disabled?: boolean
  size?: 'default' | 'sm'
  ariaLabel?: string
  placeholder?: string
  emptyLabel?: string
  onChange: (family: string) => void
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [activeFontKey, setActiveFontKey] = useState<string | null>(null)
  const listId = useId()
  const [filters, setFilters] = useState<Filters>(emptyFilters)
  const { favorites, busy: savingFavorite, setFavorite } = useFontFavorites(open)
  const favoritesOnly = useKoharuStore((state) => state.fontFavoritesOnly)
  const setFavoritesOnly = useKoharuStore((state) => state.setFontFavoritesOnly)
  const favoriteNames = useMemo(() => new Set(favorites.map(normalizeFontName)), [favorites])
  const input = useRef<HTMLInputElement>(null)
  const orderedFamilies = useMemo(
    () =>
      [...families].sort(
        (left, right) =>
          scriptRank(left) - scriptRank(right) ||
          left.name.localeCompare(right.name, undefined, {
            numeric: true,
            sensitivity: 'base',
          }),
      ),
    [families],
  )
  const facets = useMemo(() => fontFacets(orderedFamilies), [orderedFamilies])
  const results = useMemo(() => {
    const normalized = normalizeFontName(query)
    return orderedFamilies.filter((family) => {
      const matchesQuery =
        !normalized ||
        [family.name, ...family.faces.map((face) => face.postscript_name)].some((name) =>
          normalizeFontName(name).includes(normalized),
        )
      const matchesSource = !filters.source || family.sources.includes(filters.source)
      const matchesScript =
        !filters.script ||
        family.metadata.primary_script === filters.script ||
        family.metadata.scripts.includes(filters.script)
      const matchesCategory = !filters.category || family.metadata.category === filters.category
      const matchesUse =
        !filters.useCase ||
        family.metadata.classifications.includes(filters.useCase) ||
        family.metadata.use_cases.includes(filters.useCase)
      const matchesFavorite = !favoritesOnly || favoriteNames.has(normalizeFontName(family.name))
      return (
        matchesQuery &&
        matchesSource &&
        matchesScript &&
        matchesCategory &&
        matchesUse &&
        matchesFavorite
      )
    })
  }, [favoriteNames, favoritesOnly, filters, orderedFamilies, query])
  const rows = useMemo(() => {
    const starred = results
      .filter((family) => favoriteNames.has(normalizeFontName(family.name)))
      .sort((left, right) =>
        left.name.localeCompare(right.name, undefined, { sensitivity: 'base', numeric: true }),
      )
    const result: FontRow[] = emptyLabel ? [{ kind: 'empty', key: 'empty', label: emptyLabel }] : []
    if (starred.length > 0) {
      result.push({ kind: 'heading', key: 'favorites', label: t('fontPicker.favorites') })
      result.push(
        ...starred.map((family): FontRow => ({
          kind: 'font',
          key: `favorite:${normalizeFontName(family.name)}`,
          family,
        })),
      )
      if (!favoritesOnly)
        result.push({ kind: 'heading', key: 'all-fonts', label: t('fontPicker.allFonts') })
    }
    if (!favoritesOnly)
      result.push(
        ...results.map((family): FontRow => ({
          kind: 'font',
          key: `all:${normalizeFontName(family.name)}`,
          family,
        })),
      )
    return result
  }, [results, favoriteNames, favoritesOnly, t, emptyLabel])
  const selectedFamily = useMemo(
    () =>
      orderedFamilies.find((family) => normalizeFontName(family.name) === normalizeFontName(value)),
    [orderedFamilies, value],
  )
  const activeFontIndex = rows.findIndex(
    (row) => row.kind !== 'heading' && row.key === activeFontKey,
  )
  const activeIndex =
    activeFontIndex >= 0 ? activeFontIndex : rows.findIndex((row) => row.kind !== 'heading')
  const activeRow = rows[activeIndex]
  const activeFamily = activeRow?.kind === 'font' ? activeRow.family : undefined
  const updateQuery = (next: string) => {
    setQuery(next)
    setActiveFontKey(null)
  }
  const updateFilters = (next: Filters) => {
    setFilters(next)
    setActiveFontKey(null)
  }
  const chooseFont = (family: string) => {
    onChange(family)
    setOpen(false)
    setQuery('')
    setActiveFontKey(null)
  }
  const activeFilterCount = Object.values(filters).filter(Boolean).length

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
        if (!next) {
          setQuery('')
          setActiveFontKey(null)
          return
        }
        setActiveFontKey(
          rows.find(
            (row) =>
              row.kind === 'font' &&
              normalizeFontName(row.family.name) === normalizeFontName(value),
          )?.key ?? null,
        )
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
          <span className='truncate'>
            {value || emptyLabel || placeholder || t('fontPicker.choose')}
          </span>
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
              role='combobox'
              aria-autocomplete='list'
              aria-expanded={open}
              aria-controls={listId}
              aria-activedescendant={
                activeRow && activeRow.kind !== 'heading'
                  ? `${listId}-option-${activeIndex}`
                  : undefined
              }
              placeholder={t('fontPicker.search')}
              className='h-6 px-0 text-[11px]'
              onChange={(event) => updateQuery(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.nativeEvent.isComposing) return
                if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
                  if (activeIndex < 0) return
                  event.preventDefault()
                  const increment = event.key === 'ArrowDown' ? 1 : -1
                  for (
                    let index = activeIndex + increment;
                    index >= 0 && index < rows.length;
                    index += increment
                  ) {
                    const row = rows[index]
                    if (row.kind !== 'heading') {
                      setActiveFontKey(row.key)
                      break
                    }
                  }
                } else if (event.key === 'Enter' && activeRow && activeRow.kind !== 'heading') {
                  event.preventDefault()
                  chooseFont(activeRow.kind === 'font' ? activeRow.family.name : '')
                }
              }}
            />
            <InputGroupAddon align='inline-end' className='gap-0.5 pr-0.5'>
              {query && (
                <InputGroupButton
                  aria-label={t('common.clearSearch')}
                  onClick={() => updateQuery('')}
                >
                  <X />
                </InputGroupButton>
              )}
              <InputGroupButton
                aria-label={t('fontPicker.favoritesOnly')}
                aria-pressed={favoritesOnly}
                className={cn(favoritesOnly && 'bg-accent text-foreground')}
                onClick={() => {
                  setFavoritesOnly(!favoritesOnly)
                  setActiveFontKey(null)
                  input.current?.focus({ preventScroll: true })
                }}
              >
                <Star className={cn(favoritesOnly && 'fill-current text-amber-500')} />
              </InputGroupButton>
              <FontFilterMenu
                filters={filters}
                facets={facets}
                onChange={updateFilters}
                onClear={() => updateFilters(emptyFilters)}
              />
            </InputGroupAddon>
          </InputGroup>
        </div>
        <FontResultSummary
          count={results.length}
          previewFamily={activeFamily}
          activeFilterCount={activeFilterCount}
          onClear={() => updateFilters(emptyFilters)}
        />
        <Separator />
        {open && (
          <FontList
            key={`${favoritesOnly}:${query}:${filters.source}:${filters.script}:${filters.category}:${filters.useCase}`}
            rows={rows}
            activeIndex={activeIndex}
            listId={listId}
            onActiveIndexChange={(index) => {
              const row = rows[index]
              if (row && row.kind !== 'heading') setActiveFontKey(row.key)
            }}
            value={value}
            favoriteNames={favoriteNames}
            emptyHint={
              favoritesOnly && favoriteNames.size === 0 ? t('fontPicker.noFavorites') : undefined
            }
            savingFavorite={savingFavorite}
            onFavorite={(family, favorite) => {
              setFavorite(family, favorite)
              input.current?.focus({ preventScroll: true })
            }}
            onSelect={chooseFont}
          />
        )}
      </PopoverContent>
    </Popover>
  )
}

function FontFilterMenu({
  filters,
  facets,
  onChange,
  onClear,
}: {
  filters: Filters
  facets: ReturnType<typeof fontFacets>
  onChange: (filters: Filters) => void
  onClear: () => void
}) {
  const { t } = useTranslation()
  const activeFilterCount = Object.values(filters).filter(Boolean).length

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <InputGroupButton
            aria-label={
              activeFilterCount > 0
                ? t('fontPicker.filterActiveLabel', { count: activeFilterCount })
                : t('fontPicker.filter')
            }
            className={cn(activeFilterCount > 0 && 'bg-accent text-foreground')}
          />
        }
      >
        <ListFilter />
        {activeFilterCount > 0 && (
          <Badge className='h-3.5 min-w-3.5 px-1 text-[8px]'>{activeFilterCount}</Badge>
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent align='end' sideOffset={6} className='w-40'>
        <FilterSubmenu
          label={t('fontPicker.source')}
          allLabel={t('fontPicker.allSources')}
          kind='source'
          value={filters.source}
          options={facets.sources}
          onChange={(source) => onChange({ ...filters, source: source as Filters['source'] })}
        />
        <FilterSubmenu
          label={t('fontPicker.script')}
          allLabel={t('fontPicker.allScripts')}
          kind='script'
          value={filters.script}
          options={facets.scripts}
          onChange={(script) => onChange({ ...filters, script })}
        />
        <FilterSubmenu
          label={t('fontPicker.style')}
          allLabel={t('fontPicker.allStyles')}
          kind='metadata'
          value={filters.category}
          options={facets.categories}
          onChange={(category) => onChange({ ...filters, category })}
        />
        <FilterSubmenu
          label={t('fontPicker.purpose')}
          allLabel={t('fontPicker.allPurposes')}
          kind='metadata'
          value={filters.useCase}
          options={facets.uses}
          onChange={(useCase) => onChange({ ...filters, useCase })}
        />
        <DropdownMenuSeparator />
        <DropdownMenuItem
          disabled={activeFilterCount === 0}
          className='text-[11px]'
          onClick={onClear}
        >
          {t('fontPicker.clearFilters')}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function FilterSubmenu({
  label,
  allLabel,
  kind,
  value,
  options,
  onChange,
}: {
  label: string
  allLabel: string
  kind: 'source' | 'script' | 'metadata'
  value: string
  options: string[]
  onChange: (value: string) => void
}) {
  const { t } = useTranslation()
  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger className='text-[11px]'>{label}</DropdownMenuSubTrigger>
      <DropdownMenuSubContent className='max-h-60 w-44'>
        <DropdownMenuRadioGroup
          value={value || allFonts}
          onValueChange={(next) => onChange(next === allFonts ? '' : next)}
        >
          <DropdownMenuRadioItem className='text-[11px]' value={allFonts}>
            {allLabel}
          </DropdownMenuRadioItem>
          {options.map((option) => (
            <DropdownMenuRadioItem className='text-[11px]' key={option} value={option}>
              {formatFacet(kind, option, t)}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
}

function FontResultSummary({
  previewFamily,
  count,
  activeFilterCount,
  onClear,
}: {
  previewFamily?: FontFamily
  count: number
  activeFilterCount: number
  onClear: () => void
}) {
  const { t } = useTranslation()
  return (
    <div className='flex h-7 items-center justify-between gap-2 px-2 text-[9px] text-muted-foreground'>
      <span className='shrink-0'>{t('fontPicker.fontCount', { count })}</span>
      {previewFamily && (
        <div className='flex min-w-0 flex-1 items-center justify-end gap-1' aria-live='polite'>
          <FontPreviewLabel family={previewFamily} className='h-5 max-w-12 min-w-0' />
          <span className='min-w-0 truncate'>{previewFamily.name}</span>
        </div>
      )}
      {activeFilterCount > 0 ? (
        <Button variant='ghost' size='xs' className='h-5 px-1.5 text-[9px]' onClick={onClear}>
          {t('fontPicker.activeFilterCount', { count: activeFilterCount })}
          <X />
        </Button>
      ) : null}
    </div>
  )
}

function FontList({
  rows,
  activeIndex,
  listId,
  onActiveIndexChange,
  value,
  favoriteNames,
  emptyHint,
  savingFavorite,
  onFavorite,
  onSelect,
}: {
  rows: FontRow[]
  activeIndex: number
  listId: string
  onActiveIndexChange: (index: number) => void
  value: string
  favoriteNames: ReadonlySet<string>
  emptyHint?: string
  savingFavorite: boolean
  onFavorite: (family: string, favorite: boolean) => void
  onSelect: (family: string) => void
}) {
  const { t } = useTranslation()
  const list = useRef<HTMLDivElement>(null)
  const hasFavorites = rows.some((row) => row.kind === 'heading')
  const selectedIndex = rows.findIndex(
    (row) => row.kind === 'font' && normalizeFontName(row.family.name) === normalizeFontName(value),
  )
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => list.current,
    getItemKey: (index) => rows[index]?.key ?? index,
    estimateSize: (index) => (rows[index]?.kind === 'heading' ? headingHeight : rowHeight),
    overscan: 6,
    rangeExtractor: (range) => {
      const visible = defaultRangeExtractor(range)
      return activeIndex >= 0 && !visible.includes(activeIndex)
        ? [...visible, activeIndex].sort((left, right) => left - right)
        : visible
    },
    initialOffset: hasFavorites
      ? 0
      : Math.max(
          0,
          selectedIndex * rowHeight - Math.floor((listHeight / rowHeight - 1) / 2) * rowHeight,
        ),
    initialRect: { width: 240, height: listHeight },
    observeElementRect: (instance, callback) =>
      observeElementRect(instance, (rect) =>
        callback({ width: rect.width || 240, height: rect.height || listHeight }),
      ),
  })

  useEffect(() => {
    if (activeIndex >= 0 && activeIndex < rows.length)
      virtualizer.scrollToIndex(activeIndex, { align: 'auto' })
  }, [activeIndex, rows.length, virtualizer])

  return (
    <ScrollArea
      viewportRef={list}
      role='listbox'
      id={listId}
      aria-label={t('fontPicker.fonts')}
      className='relative'
      viewportClassName='overflow-x-hidden'
      style={{
        height:
          rows.length === 0
            ? rowHeight * 2
            : Math.min(
                listHeight,
                Math.max(
                  rowHeight,
                  rows.reduce(
                    (height, row) => height + (row.kind === 'heading' ? headingHeight : rowHeight),
                    0,
                  ),
                ),
              ),
      }}
    >
      <div className='relative' style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const row = rows[virtualRow.index]
          if (!row) return null
          if (row.kind === 'heading') {
            return (
              <div
                key={virtualRow.key}
                role='presentation'
                className='absolute inset-x-0 top-0 flex h-6 items-center bg-muted/50 px-2 text-[10px] font-medium text-muted-foreground'
                style={{ transform: `translateY(${virtualRow.start}px)` }}
              >
                {row.label}
              </div>
            )
          }
          if (row.kind === 'empty') {
            return (
              <button
                key={virtualRow.key}
                type='button'
                role='option'
                id={`${listId}-option-${virtualRow.index}`}
                aria-selected={!value}
                data-highlighted={activeIndex === virtualRow.index}
                className={cn(
                  'absolute inset-x-0 top-0 flex h-9.5 items-center px-2 text-left text-[11px] hover:bg-accent focus-visible:outline-ring',
                  (!value || activeIndex === virtualRow.index) && 'bg-accent',
                )}
                style={{ transform: `translateY(${virtualRow.start}px)` }}
                onPointerMove={() => onActiveIndexChange(virtualRow.index)}
                onClick={() => onSelect('')}
              >
                {row.label}
              </button>
            )
          }
          const family = row.family
          const starred = favoriteNames.has(normalizeFontName(family.name))
          const selected = normalizeFontName(family.name) === normalizeFontName(value)
          const source = t(
            family.sources.includes('bundled')
              ? 'fontPicker.sources.bundled'
              : 'fontPicker.sources.system',
          )
          return (
            <div
              key={virtualRow.key}
              className={cn(
                'absolute inset-x-0 top-0 flex h-9.5 w-full items-center focus-within:bg-accent hover:bg-accent',
                (selected || activeIndex === virtualRow.index) &&
                  'bg-accent text-accent-foreground',
              )}
              style={{ transform: `translateY(${virtualRow.start}px)` }}
              onPointerMove={() => onActiveIndexChange(virtualRow.index)}
            >
              <button
                type='button'
                role='option'
                id={`${listId}-option-${virtualRow.index}`}
                data-highlighted={activeIndex === virtualRow.index}
                aria-label={t('fontPicker.fontLabel', { family: family.name, source })}
                aria-selected={selected}
                className='flex h-full min-w-0 flex-1 flex-col justify-center gap-px px-2 text-left focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring'
                onClick={() => onSelect(family.name)}
              >
                <FontPreviewLabel family={family} className='h-4.5 w-full min-w-0' />
                <div className='flex min-w-0 items-center justify-between gap-2 text-[8px] leading-none text-muted-foreground'>
                  <span className='min-w-0 truncate'>{family.name}</span>
                  <span className='shrink-0'>{source}</span>
                </div>
              </button>
              <Button
                type='button'
                variant='ghost'
                size='icon'
                className='mr-1 size-7 shrink-0'
                aria-label={t(starred ? 'fontPicker.removeFavorite' : 'fontPicker.addFavorite', {
                  family: family.name,
                })}
                aria-pressed={starred}
                disabled={savingFavorite}
                onClick={() => onFavorite(family.name, !starred)}
              >
                <Star className={cn('size-3.5', starred && 'fill-current text-amber-500')} />
              </Button>
            </div>
          )
        })}
      </div>
      {rows.length === 0 && (
        <Empty role='status' className='absolute inset-1 rounded-md p-2'>
          <EmptyTitle className='text-[11px] text-muted-foreground'>
            {emptyHint ?? t('fontPicker.noResults')}
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

function fontFacets(families: FontFamily[]) {
  return {
    sources: unique(families.flatMap((family) => family.sources)),
    scripts: unique(
      families.flatMap((family) => [
        ...(family.metadata.primary_script ? [family.metadata.primary_script] : []),
        ...family.metadata.scripts,
      ]),
    ),
    categories: unique(
      families.flatMap((family) => (family.metadata.category ? [family.metadata.category] : [])),
    ),
    uses: unique(
      families.flatMap((family) => [
        ...family.metadata.classifications,
        ...family.metadata.use_cases,
      ]),
    ),
  }
}

function unique(values: string[]): string[] {
  return [...new Set(values)].sort((left, right) =>
    formatMetadata(left).localeCompare(formatMetadata(right), undefined, {
      numeric: true,
      sensitivity: 'base',
    }),
  )
}

function formatMetadata(value: string): string {
  return value
    .replaceAll('_', ' ')
    .replaceAll('-', ' ')
    .replace(/\b\w/g, (letter) => letter.toUpperCase())
}

function formatFacet(kind: 'source' | 'script' | 'metadata', value: string, t: TFunction): string {
  if (kind === 'script') {
    return t(`fontPicker.scripts.${value.toLowerCase()}`, { defaultValue: formatMetadata(value) })
  }
  if (kind === 'source') {
    return t(`fontPicker.sources.${value}`, { defaultValue: formatMetadata(value) })
  }
  return formatMetadata(value)
}

function normalizeFontName(value: string): string {
  return value.normalize('NFKC').trim().replace(/\s+/g, ' ').toLowerCase()
}

function scriptRank(family: FontFamily): number {
  const script = family.metadata.primary_script
  if (script === 'latn') return 0
  if (script === 'cyrl') return 1
  if (script === 'hani' || script === 'hira' || script === 'kana') return 2
  if (script === 'hang') return 3
  if (script === 'arab') return 4
  if (script === 'hebr') return 5
  if (['deva', 'beng', 'guru', 'gujr', 'taml', 'telu', 'knda', 'mlym'].includes(script ?? '')) {
    return 6
  }
  if (script === 'thai') return 7
  return fontNameScript(family.name)
}

function fontNameScript(name: string): number {
  for (const character of name) {
    if (/\p{Script=Latin}/u.test(character)) return 0
    if (/\p{Script=Cyrillic}/u.test(character)) return 1
    if (/\p{Script=Han}|\p{Script=Hiragana}|\p{Script=Katakana}/u.test(character)) return 2
    if (/\p{Script=Hangul}/u.test(character)) return 3
    if (/\p{Script=Arabic}/u.test(character)) return 4
    if (/\p{Script=Hebrew}/u.test(character)) return 5
    if (
      /\p{Script=Devanagari}|\p{Script=Bengali}|\p{Script=Gurmukhi}|\p{Script=Gujarati}|\p{Script=Tamil}|\p{Script=Telugu}|\p{Script=Kannada}|\p{Script=Malayalam}/u.test(
        character,
      )
    ) {
      return 6
    }
    if (/\p{Script=Thai}/u.test(character)) return 7
  }
  return 8
}
