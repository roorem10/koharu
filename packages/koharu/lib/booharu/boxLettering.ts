'use client'

import { useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { categoryName, letteringCategories } from '@/lib/booharu/categories'
import i18n from '@/lib/i18n'
import { pagesKey, queryClient } from '@/lib/queries'
import {
  commands,
  type BlockLettering,
  type CategoryCounts,
  type LayerId,
  type LetteringCategory,
  type PageId,
} from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

/** What a block's category picker offers: Automatic, which leaves the category to the block's
 * page, or a category the user picks for it. */
export type LetteringChoice = LetteringCategory | 'automatic'

/** A block's picker's choices, Automatic first, then every category in Settings' order. */
export const letteringChoices: readonly LetteringChoice[] = ['automatic', ...letteringCategories]

/** The string key of `choice`'s name. */
export function choiceName(choice: LetteringChoice) {
  return choice === 'automatic' ? 'booharu.lettering.automatic' : categoryName(choice)
}

/** What a block's picker shows as chosen: the category the user picked for it, or Automatic for
 * none, a model's label included, which a user's Automatic clears. */
export function chosen(lettering: BlockLettering): LetteringChoice {
  return lettering.label?.source === 'user' ? lettering.label.category : 'automatic'
}

/** `share`, from 0 to 1, as a whole percentage, as a bubble's measures show. */
export function percent(share: number): number {
  return Math.round(share * 100)
}

/** The key of `page`'s blocks' lettering, under Story's key for the page, so every edit, undo or
 * redo, which reads Story's pages again, reads it again too: a block's category follows its label,
 * its role, its source and its balloon, any of which a change can touch. */
export function letteringKey(page: PageId) {
  return [...pagesKey, page, 'booharu', 'lettering'] as const
}

/** Reads the lettering of every block on `page` now, as a command does, a failure showing its error
 * and rejecting, and keeps it for the views showing the page. */
export async function fetchLettering(page: PageId): Promise<void> {
  const lettering = await call(commands.booharuGetLettering, page)
  queryClient.setQueryData(letteringKey(page), lettering)
}

function byBlock(lettering: BlockLettering[]): Map<LayerId, BlockLettering> {
  return new Map(lettering.map((entry) => [entry.block, entry]))
}

/** The lettering of every block on `page`, by block, read while a view showing it is mounted and
 * again after every change; none while `page` is. A failed read shows no error toast: one of a page
 * just removed fails, as the page's views go, and they then show nothing of it. Box details says
 * why one failed otherwise, as for a `[booharu-lettering]` the settings refuse. */
export function useLettering(page: PageId | null | undefined) {
  return useQuery({
    queryKey: letteringKey(page ?? ''),
    queryFn: () => commands.booharuGetLettering(page ?? ''),
    enabled: !!page,
    select: byBlock,
  })
}

/** `block`'s lettering on `page`, as a view shows it. `lettering` is undefined while it's being
 * read, and when the read failed with nothing read before, and null when the page has no such
 * block; `error` is that failure's message, and null otherwise, as when a read again fails and what
 * was read before stays. */
export function useBlockLettering(
  page: PageId | null | undefined,
  block: LayerId,
): { lettering: BlockLettering | null | undefined; error: string | null } {
  const { data, error } = useLettering(page)
  if (data) return { lettering: data.get(block) ?? null, error: null }
  if (error === null) return { lettering: undefined, error: null }
  // The desktop rejects with its message, a string.
  return { lettering: undefined, error: error instanceof Error ? error.message : String(error) }
}

/** Sets the lettering category of `blocks` as the user picks it, `choice`, Automatic clearing their
 * labels, as one undo step: while Pick fonts automatically is on, each translation takes its new
 * category's font, even one changed by hand. Locked blocks are left out, and a toast says how many;
 * the change arrives as every change does. A failure shows its error, as every command's does, and
 * rejects. */
export async function setLetteringCategory(
  blocks: LayerId[],
  choice: LetteringChoice,
): Promise<CategoryCounts> {
  const counts = await call(
    commands.booharuSetLetteringCategory,
    blocks,
    choice === 'automatic' ? null : choice,
  )
  if (counts.locked > 0) {
    toast.add({
      type: 'info',
      title: i18n.t('booharu.lettering.keptLocked', { count: counts.locked }),
    })
  }
  return counts
}
