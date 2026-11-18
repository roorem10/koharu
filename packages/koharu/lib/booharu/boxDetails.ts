'use client'

import { create } from 'zustand'

import { call } from '@/lib/backend'
import { projectKey, queryClient, type ProjectView } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type LayerId, type PageId } from '@koharu/ipc/protocol'

/** How long Show in Story waits for the Story row it brings into view to show, in milliseconds. */
const revealWait = 2000

interface BoxDetailsState {
  /** The text Box details shows, and its page; null while it's closed. */
  shown: { page: PageId; layer: LayerId } | null
  /** The text whose Story row Show in Story brings into view, which the row's tag does once it
   * shows, if it shows before `until`, a `performance.now()` time. */
  reveal: { layer: LayerId; until: number } | null
}

/** Box details' state, the interface's own, apart from the project and Koharu's store. */
export const useBoxDetails = create<BoxDetailsState>()(() => ({ shown: null, reveal: null }))

export function openBoxDetails(page: PageId, layer: LayerId): void {
  useBoxDetails.setState({ shown: { page, layer }, reveal: null })
}

export function closeBoxDetails(): void {
  useBoxDetails.setState({ shown: null })
}

/** Closes Box details and shows its text in Story: the Story tab, as the right pane's tabs switch,
 * and the text selected, its page shown first when the canvas shows another, as focusing a Story
 * row does; its row's tag then scrolls it into view. */
export function showInStory(page: PageId, layer: LayerId): void {
  useBoxDetails.setState({
    shown: null,
    reveal: { layer, until: performance.now() + revealWait },
  })
  const { showPanel, selectPages, selectLayers } = useKoharuStore.getState()
  showPanel('story')
  if (queryClient.getQueryData<ProjectView | null>(projectKey)?.page === page) {
    selectLayers([layer])
    return
  }
  selectPages([page])
  selectLayers([])
  void call(commands.show, page)
    .then(() => useKoharuStore.getState().selectLayers([layer]))
    .catch(() => undefined)
}
