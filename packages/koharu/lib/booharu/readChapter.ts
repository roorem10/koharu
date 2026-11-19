'use client'

import { useChapterContext } from '@/lib/booharu/context'
import { readsByDefault, setReadChapter, useReader, useReadSwitches } from '@/lib/booharu/reading'
import { useProject } from '@/lib/queries'

/** The Run menu's Read chapter in the open project: whether it's on, as switched or else by
 * default; `reader`, the model it would read with; and `set`, which switches it. Apart from the
 * reading rules, which the run settings load before the chapter's queries can. */
export function useReadChapter() {
  const project = useProject().data?.name ?? null
  const chapter = useChapterContext().data
  const reader = useReader()
  const switched = useReadSwitches((state) => (project === null ? undefined : state.on[project]))
  return {
    project,
    reader,
    on: switched ?? readsByDefault(chapter, reader),
    set: (on: boolean) => {
      if (project !== null) setReadChapter(project, on)
    },
  }
}
