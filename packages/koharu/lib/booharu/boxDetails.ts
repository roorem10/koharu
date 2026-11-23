'use client'

import { create } from 'zustand'

import { call } from '@/lib/booharu/call'
import { projectKey, queryClient, type ProjectView } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type LayerId, type PageId } from '@koharu/ipc/protocol'

/** How long Show in Story waits for the Story row it brings into view to show, in milliseconds. */
const revealWait = 2000

interface BoxDetailsState {
  /** The text Box details shows, its page, and the section it opened on, which takes the focus;
   * null while it's closed. */
  shown: { page: PageId; layer: LayerId; section?: string } | null
  /** The text whose Story row Show in Story brings into view, which the row's tag does once it
   * shows, if it shows before `until`, a `performance.now()` time. */
  reveal: { layer: LayerId; until: number } | null
}

/** Box details' state, the interface's own, apart from the project and Koharu's store. */
export const useBoxDetails = create<BoxDetailsState>()(() => ({ shown: null, reveal: null }))

/** Opens Box details on `layer` of `page`, the focus on its section `section` when given, as the
 * box action that changes a text's category opens it on Lettering. */
export function openBoxDetails(page: PageId, layer: LayerId, section?: string): void {
  useBoxDetails.setState({ shown: { page, layer, section }, reveal: null })
}

export function closeBoxDetails(): void {
  useBoxDetails.setState({ shown: null })
}

/** Closes Box details and shows its text in Story: the Story tab, as the right pane's tabs switch,
 * and the text selected as `showLayer` selects it; its row's tag then scrolls it into view. */
export function showInStory(page: PageId, layer: LayerId): void {
  useBoxDetails.setState({
    shown: null,
    reveal: { layer, until: performance.now() + revealWait },
  })
  useKoharuStore.getState().showPanel('story')
  void showLayer(page, layer).catch(() => undefined)
}

/** Counts the jumps `showLayer` makes, so only the last one selects its text. */
let jumps = 0

/** The jumps' page shows, each sent once the one before has ended, so the canvas ends on the last
 * jump's page, and how many are under way, while which the page the canvas shows is about to
 * change. */
let shows: Promise<unknown> = Promise.resolve()
let showing = 0

/** Selects `layer` of `page`, as focusing its Story row does, whichever tab shows: at once when the
 * canvas shows `page` and no jump is showing another; else with `page` alone selected among the
 * pages and no layer until the canvas shows it, then the text. A later jump, or a page or text
 * selected meanwhile, wins over a page shown late, which then selects nothing. */
export async function showLayer(page: PageId, layer: LayerId): Promise<void> {
  const jump = ++jumps
  const { selectPages, selectLayers } = useKoharuStore.getState()
  if (!showing && queryClient.getQueryData<ProjectView | null>(projectKey)?.page === page) {
    selectLayers([layer])
    return
  }
  selectPages([page])
  selectLayers([])
  showing += 1
  const shown = shows.then(() => call(commands.show, page, null))
  shows = shown.catch(() => undefined)
  try {
    await shown
  } finally {
    showing -= 1
  }
  const now = useKoharuStore.getState()
  const untouched =
    now.selectedPages.length === 1 && now.selectedPages[0] === page && !now.selectedLayers.length
  if (jump === jumps && untouched) now.selectLayers([layer])
}
