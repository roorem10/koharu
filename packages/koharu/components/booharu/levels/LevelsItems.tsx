'use client'

import { useMemo, type ComponentProps, type ComponentType } from 'react'
import { useTranslation } from 'react-i18next'

import { menuPages, openLevels, restoreOriginals, useLevelsStatus } from '@/lib/booharu/levels'
import { useKoharuStore } from '@/lib/store'
import type { Page, PageId } from '@koharu/ipc/protocol'
import type { DropdownMenuItem } from '@koharu/ui/components/dropdown-menu'

type ItemProps = ComponentProps<typeof DropdownMenuItem>

/** A page's menu's levels items, after a `Separator`, each an `Item` of the menu with `className`:
 * Fix page levels…, which opens the levels dialog on the pages the menu acts on, and Restore
 * original scan, while the pages' status says any of them has its Original scan, which puts those
 * back at once, as one undo step, asking nothing. The pages are the selection when `page`, the page
 * the menu is for, is in it, otherwise that page alone. */
export function LevelsPageItems({
  page,
  Item,
  Separator,
  className,
}: {
  page: PageId
  Item: ComponentType<ItemProps>
  Separator: ComponentType
  className?: string
}) {
  const { t } = useTranslation()
  const selected = useKoharuStore((state) => state.selectedPages)
  const pages = useMemo(() => menuPages(page, selected), [page, selected])
  const status = useLevelsStatus(pages).data
  const originals = (status ?? []).filter((each) => each.original).map((each) => each.page)

  return (
    <>
      <Separator />
      <Item className={className} onClick={() => openLevels(pages, 'selected')}>
        {t('booharu.levels.fix')}
      </Item>
      {originals.length > 0 && (
        <Item
          className={className}
          onClick={() => void restoreOriginals(originals).catch(() => undefined)}
        >
          {t('booharu.levels.restore')}
        </Item>
      )}
    </>
  )
}

/** The Edit menu's Fix page levels…, an `Item` of the menu, which opens the levels dialog on every
 * page, with the pages selected in the page list as its Selected pages. Disabled while `page`, the
 * page the canvas shows, is null, as it is while reading. */
export function LevelsMenuItem({
  page,
  Item,
}: {
  page: Page | null
  Item: ComponentType<ItemProps>
}) {
  const { t } = useTranslation()
  return (
    <Item
      disabled={page === null}
      onClick={() => openLevels(useKoharuStore.getState().selectedPages, 'all')}
    >
      {t('booharu.levels.fix')}
    </Item>
  )
}
