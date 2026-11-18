'use client'

import { Star } from 'lucide-react'
import type { ReactElement, RefObject } from 'react'
import { useTranslation } from 'react-i18next'

import type { BooharuFontProps, BooharuListProps, FontRow } from '@/lib/booharu/fonts'
import { Button } from '@koharu/ui/components/button'
import { InputGroupAddon, InputGroupButton } from '@koharu/ui/components/input-group'
import { cn } from '@koharu/ui/lib/utils'

/** The search line's switch in Koharu's font picker that shows only the favourites, given
 * `onFavoritesOnlyChange`. It gives the search line the focus back. */
export function FavoritesOnlySwitch({
  booharu: { favoritesOnly = false, onFavoritesOnlyChange },
  search,
}: {
  booharu: BooharuFontProps
  search: RefObject<HTMLInputElement | null>
}) {
  const { t } = useTranslation()
  if (!onFavoritesOnlyChange) return null
  const label = t('booharu.fonts.favoritesOnly')
  return (
    <InputGroupAddon align='inline-end' className='pr-0.5'>
      <InputGroupButton
        aria-label={label}
        title={label}
        aria-pressed={favoritesOnly}
        className={cn(favoritesOnly && 'bg-accent text-foreground')}
        onClick={() => {
          onFavoritesOnlyChange(!favoritesOnly)
          search.current?.focus({ preventScroll: true })
        }}
      >
        <Star className={cn(favoritesOnly && 'fill-current text-amber-500')} />
      </InputGroupButton>
    </InputGroupAddon>
  )
}

/** A family's option in Koharu's font list, `option`, and right after it, given
 * `onToggleFavorite`, its star over the end of its row, `top` pixels down the list, so the Tab key
 * reaches each star in turn with its family. The star stars its family or unstars it and gives the
 * search line the focus back, so the arrows go on from there, and the focus stays in the picker
 * when the star's row goes. Moving the pointer over a star, or the focus to it, marks its row, as
 * moving either to the row does. */
export function WithFavoriteStar({
  option,
  row,
  top,
  booharu: { onToggleFavorite, search },
  onActive,
}: {
  option: ReactElement
  row: Extract<FontRow, { family: unknown }>
  top: number
  booharu: BooharuListProps
  onActive: (key: string) => void
}) {
  const { t } = useTranslation()
  if (!onToggleFavorite) return option
  const family = row.family.name
  return (
    <>
      {option}
      <Button
        type='button'
        variant='ghost'
        size='icon-xs'
        aria-label={t(row.starred ? 'booharu.fonts.removeFavorite' : 'booharu.fonts.addFavorite', {
          family,
        })}
        className='absolute top-[7px] right-1.5 text-muted-foreground hover:text-foreground'
        style={{ transform: `translateY(${top}px)` }}
        onPointerMove={(event) => {
          if (event.movementX || event.movementY) onActive(row.key)
        }}
        onFocus={() => onActive(row.key)}
        onClick={() => {
          onToggleFavorite(family, !row.starred)
          search.current?.focus({ preventScroll: true })
        }}
      >
        <Star className={cn('size-3.5', row.starred && 'fill-current text-amber-500')} />
      </Button>
    </>
  )
}

/** A section's heading in Koharu's font list, `top` pixels down it, as ModelPicker heads each
 * vendor's models. */
export function FontHeading({ heading, top }: { heading: 'favorites' | 'allFonts'; top: number }) {
  const { t } = useTranslation()
  return (
    <div
      role='presentation'
      className='absolute inset-x-0 top-0 flex h-6 items-end truncate px-2 pb-0.5 text-[10px] font-medium text-muted-foreground'
      style={{ transform: `translateY(${top}px)` }}
    >
      {t(`booharu.fonts.${heading}`)}
    </div>
  )
}
