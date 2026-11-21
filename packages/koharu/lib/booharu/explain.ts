'use client'

import { queryOptions, useQuery } from '@tanstack/react-query'
import { create } from 'zustand'

import { openBoxDetails, useBoxDetails } from '@/lib/booharu/boxDetails'
import { failure } from '@/lib/booharu/context'
import { openProject } from '@/lib/booharu/reading'
import { plainText, type BlockLayer } from '@/lib/document'
import { queryClient } from '@/lib/queries'
import { commands, type ExplainAnswer, type LayerId, type PageId } from '@koharu/ipc/protocol'

/** Box details' section that explains a line. */
export const explainSection = 'explain'

/** Where a line's explanation stands: asked for, answered, or failed with the desktop's message,
 * such as that Stop or closing the project stopped it. */
export type Explanation =
  | { state: 'explaining' }
  | { state: 'answered'; answer: ExplainAnswer }
  | { state: 'failed'; error: string }

/** Each line's explanation by its layer, in the project `project`: kept while the project stays
 * open, so Box details closed while one is asked for loses no answer, and dropped as another opens,
 * whose lines are its own. */
export const useExplanations = create<{
  project: string | null
  lines: Record<LayerId, Explanation>
}>()(() => ({ project: null, lines: {} }))

/** The explanation of the line `layer` in the open project, `project`; none before one is asked
 * for. */
export function useExplanation(project: string | null, layer: LayerId): Explanation | undefined {
  return useExplanations((state) =>
    project !== null && state.project === project ? state.lines[layer] : undefined,
  )
}

/** The answer of `explanation` while it's about `block` as it is now, else none: an answer is
 * dropped once the line's source or translation changed since it was asked, as by its Use this. */
export function answerFor(
  explanation: Explanation | undefined,
  block: BlockLayer,
): ExplainAnswer | null {
  if (explanation?.state !== 'answered') return null
  const { answer } = explanation
  return answer.source === plainText(block.source) &&
    answer.translation === plainText(block.translation)
    ? answer
    : null
}

/** Asks the model Explain this line uses about the line `layer` of the open project, unless it's
 * asked about already, and keeps how it went, which the line's card shows: no toast, as the card
 * shows a failure with Try again. The desktop adds the answer to the agent's chat as a note. */
export async function explainLine(layer: LayerId): Promise<void> {
  const project = openProject()
  if (project === null) return
  const held = useExplanations.getState()
  const lines = held.project === project ? held.lines : {}
  if (lines[layer]?.state === 'explaining') return
  useExplanations.setState({ project, lines: { ...lines, [layer]: { state: 'explaining' } } })
  let explanation: Explanation
  try {
    explanation = { state: 'answered', answer: await commands.booharuExplainLine(layer) }
  } catch (error) {
    explanation = { state: 'failed', error: failure(error) }
  }
  useExplanations.setState((state) =>
    state.project === project ? { lines: { ...state.lines, [layer]: explanation } } : state,
  )
}

/** Which models Read chapter and Explain this line use, as the desktop answers from the settings
 * and Koharu's models. A failed read shows no toast: Explain says why as it fails the same way. */
const readingModelQuery = queryOptions({
  queryKey: ['booharu', 'reading-model'] as const,
  queryFn: () => commands.booharuGetReadingModel(),
  meta: { booharuQuiet: true },
})

/** The model Explain this line would use now, by its reference; none when none is set, which
 * disables it, and undefined until known, or when it couldn't be read. Read again each time a view
 * showing it opens, as Settings or the agent's model picker may have changed it since. */
export function useExplainModel(): { model: string | null | undefined; known: boolean } {
  const { data, isError } = useQuery({
    ...readingModelQuery,
    refetchOnMount: 'always',
    retryOnMount: false,
  })
  return { model: data?.explain, known: data !== undefined || isError }
}

/** Explain this line, from Story's right-click menu and the box actions: Box details opens on the
 * line `block` of `page` at its explanation at once, as the desktop may take a while to say which
 * model explains lines, listing every vendor's models. Once it has, the explanation is asked for
 * when a model explains lines, Box details still shows the line, and the line has none going or
 * shown as it is now; with no model, the card says which settings choose one. Settles once it's
 * asked for, or won't be, the answer still on its way. */
export async function openExplanation(page: PageId, block: BlockLayer): Promise<void> {
  openBoxDetails(page, block.id, explainSection)
  const reader = await queryClient
    .fetchQuery({ ...readingModelQuery, staleTime: 0 })
    .catch(() => null)
  const shown = useBoxDetails.getState().shown
  if (!reader?.explain || shown?.page !== page || shown.layer !== block.id) return
  const { project, lines } = useExplanations.getState()
  const held = project === openProject() ? lines[block.id] : undefined
  if (held?.state === 'explaining' || answerFor(held, block)) return
  void explainLine(block.id)
}
