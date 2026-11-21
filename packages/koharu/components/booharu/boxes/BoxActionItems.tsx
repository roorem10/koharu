'use client'

import { useQuery } from '@tanstack/react-query'
import { useMemo, type ComponentProps, type ComponentType } from 'react'
import { useTranslation } from 'react-i18next'

import {
  boxActions,
  joinPage,
  joinsPage,
  runBoxAction,
  selectedBoxes,
  type BoxSelection,
} from '@/lib/booharu/boxActions'
import { useLettering } from '@/lib/booharu/boxLettering'
import { pageKey, usePageStory } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type LayerId, type Page, type PageId } from '@koharu/ipc/protocol'
import { DropdownMenuItem, DropdownMenuSeparator } from '@koharu/ui/components/dropdown-menu'

type ItemProps = ComponentProps<typeof DropdownMenuItem>

/** The box actions for `selection`, each an `Item` of the menu it sits in, disabled where it can't
 * act, or while the selection is still unknown. The page's lettering is read while the menu shows,
 * so an action that needs a block's category, Add subtitle, follows it. */
function ActionItems({
  selection,
  Item,
}: {
  selection: BoxSelection | null
  Item: ComponentType<ItemProps>
}) {
  const { t } = useTranslation()
  const lettering = useLettering(selection?.page).data
  const lettered = useMemo(() => selection && { ...selection, lettering }, [selection, lettering])
  return (
    <>
      {boxActions.map((action) => (
        <Item
          key={action.id}
          disabled={!lettered || !action.enabled(lettered)}
          onClick={() => {
            if (lettered) runBoxAction(action, lettered)
          }}
        >
          {t(action.label)}
        </Item>
      ))}
    </>
  )
}

/** A Story row's menu items: its text's box actions, above Delete, Koharu's items as its Delete. */
export function StoryBoxActions({ page, layer }: { page: PageId; layer: LayerId }) {
  // The page as its Story section read it, so the text's balloon and locks are known.
  const layers = usePageStory(page).data?.page.layers
  const selection = useMemo(
    () => (layers ? { page, boxes: selectedBoxes(layers, [layer]) } : null),
    [page, layer, layers],
  )
  return (
    <>
      <ActionItems selection={selection} Item={DropdownMenuItem} />
      <DropdownMenuSeparator />
    </>
  )
}

/** The box actions for the selection on `page`, the page the canvas shows, each an `Item` of the
 * menu they sit in: the Edit menu's, and the box menu a right-click on the canvas opens. */
export function SelectionBoxActions({
  page,
  Item,
}: {
  page: Page | null
  Item: ComponentType<ItemProps>
}) {
  const selected = useKoharuStore((state) => state.selectedLayers)
  const selection = useMemo(
    () => (page ? { page: page.id, boxes: selectedBoxes(page.layers, selected) } : null),
    [page, selected],
  )
  return <ActionItems selection={selection} Item={Item} />
}

/** The Edit menu's box items, after a `Separator`, each an `Item` of the menu: the box actions for
 * the selection on `page`, the page the canvas shows, then joining every split balloon there,
 * disabled while a lock in any of them would refuse the join. */
export function BoxMenuItems({
  page,
  Item,
  Separator,
}: {
  page: Page | null
  Item: ComponentType<ItemProps>
  Separator: ComponentType
}) {
  const { t } = useTranslation()
  const split = useSplitBalloons(page?.id ?? null)
  const joins = page !== null && split !== undefined && joinsPage(page.layers, split)
  return (
    <>
      <Separator />
      <SelectionBoxActions page={page} Item={Item} />
      <Item
        disabled={!joins}
        onClick={() => {
          if (page && split && joins)
            void joinPage(page.id, page.layers, split).catch(() => undefined)
        }}
      >
        {t('booharu.boxes.joinPage')}
      </Item>
    </>
  )
}

/** The balloons of `page` whose text is split over several blocks, as the desktop finds them, read
 * while the menu shows. Keyed under Koharu's shown-page key, so every change, which reads the shown
 * page again, reads them again too; a failed read shows its error, as every command's does. */
function useSplitBalloons(page: PageId | null) {
  return useQuery({
    queryKey: [...pageKey, 'booharu', 'split-balloons', page],
    queryFn: () => commands.booharuFindSplitBalloons(page),
    enabled: page !== null,
  }).data
}
