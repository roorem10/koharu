'use client'

import { useEffect } from 'react'
import { create } from 'zustand'

import { showLayer } from '@/lib/booharu/boxDetails'
import { BLACK, ownColor, seedGradient, spanStyles } from '@/lib/booharu/paint'
import { chosenText, type WordRange } from '@/lib/booharu/words'
import { letteringOf } from '@/lib/document'
import { projectKey, queryClient, type ProjectView } from '@/lib/queries'
import { useKoharuStore, type CanvasTool } from '@/lib/store'
import type { Gradient, Layer, LayerId, PageId, Text, Typed } from '@koharu/ipc/protocol'

// Booharu's Text effects popover in the type bar (S12-k): whether it's open, which its fx button,
// FX… in a box's menu and a box's fx tag open, closing when the selection empties or the page
// changes; and what its gradient fill reads and writes over the bar's targets.

interface FxState {
  /** Whether the popover is open. */
  open: boolean
  /** The page it opened on, which it closes on leaving; none until the type bar shows it. */
  page: PageId | null
}

/** The Text effects popover's state, the interface's own. */
export const useFx = create<FxState>()(() => ({ open: false, page: null }))

/** Opens Text effects over the type bar's targets, on the page the bar shows. */
export function openFx(): void {
  useFx.setState({ open: true, page: null })
}

/** Closes Text effects. */
export function closeFx(): void {
  useFx.setState({ open: false, page: null })
}

/** The tools the type bar shows for, as TypeBar lists them. */
const BAR_TOOLS: readonly CanvasTool[] = ['select', 'direct_select', 'text']

/** Opens Text effects over `layers` of `page`, as FX… in a box's menu (S12-l) and a box's fx tag
 * (S12-j) do: selects them first unless the canvas shows them selected, through showLayer, which
 * shows their page first when the canvas shows another; switches to the Select tool unless one of
 * the type bar's tools is on; and opens the popover, which shows once the type bar does. A page or
 * a layer selected meanwhile wins, and nothing opens. */
export async function openFxOn(page: PageId, layers: LayerId[]): Promise<void> {
  const selected = () => {
    const { selectedLayers } = useKoharuStore.getState()
    return layers.every((layer) => selectedLayers.includes(layer))
  }
  const [first, ...rest] = layers
  if (!first) return
  const shown = queryClient.getQueryData<ProjectView | null>(projectKey)?.page === page
  if (!shown || !selected()) {
    await showLayer(page, first).catch(() => undefined)
    // The others join the first once it alone is selected, as showLayer leaves it.
    const { selectedLayers, selectLayers } = useKoharuStore.getState()
    if (rest.length && selectedLayers.length === 1 && selectedLayers[0] === first) {
      selectLayers(layers)
    }
    if (!selected()) return
  }
  const { tool, setTool } = useKoharuStore.getState()
  if (!BAR_TOOLS.includes(tool)) setTool('select')
  openFx()
}

/** Whether Text effects shows on `page`, the page the type bar shows: open, and opened there. It
 * closes when the bar shows another page, and when nothing is selected or typed any more, as the
 * bar, with what calls this, goes. */
export function useFxOpen(page: PageId | undefined): boolean {
  const { open, page: openedOn } = useFx()
  useEffect(() => {
    if (!open || !page) return
    if (openedOn === null) useFx.setState({ page })
    else if (openedOn !== page) closeFx()
  }, [open, openedOn, page])
  // The store tells its listeners before the bar goes, so this hears the selection empty.
  useEffect(
    () =>
      useKoharuStore.subscribe((state) => {
        if (!useFx.getState().open) return
        if (state.selectedLayers.length === 0 && state.typing === null) closeFx()
      }),
    [],
  )
  return open && (openedOn === null || openedOn === page)
}

/** The texts whose characters the type bar styles, as it reads what they share: the words chosen
 * in Story, as a text of their own, or every target's lettering; none while the canvas types, as
 * the canvas tells what its characters share. */
export function styledTexts(
  targets: Layer[],
  words: WordRange | null,
  typing: Typed | null,
): Text[] | null {
  if (typing) return null
  const texts = targets.flatMap((layer) => letteringOf(layer) ?? [])
  if (!words) return texts
  const [lettering] = texts
  return lettering ? [chosenText(lettering, words)] : []
}

/** Whether every character of `text` shows a gradient. */
function allGradient(text: Text): boolean {
  return spanStyles(text).every((style) => style.fill_gradient)
}

/** Whether a character of `text` shows a gradient. */
export function showsGradient(text: Text | null): boolean {
  return text !== null && spanStyles(text).some((style) => style.fill_gradient)
}

/** The gradient Gradient fill gives each of `targets` turning on, by layer: the gradient its
 * style has already, which words given a colour of their own hide, or else one seeded from its own
 * flat colour; none for a target every character of which shows a gradient already, which keeps
 * its own. A block with no translation yet seeds from black, as it'll letter. */
export function targetSeeds(targets: Layer[]): Map<LayerId, Gradient> {
  const seeds = new Map<LayerId, Gradient>()
  for (const layer of targets) {
    const text = letteringOf(layer)
    if (text && allGradient(text)) continue
    const own = text?.style?.chars?.fill_gradient
    seeds.set(layer.id, own ?? seedGradient(text ? ownColor(text) : BLACK))
  }
  return seeds
}

/** The gradient every one of `seeds` is, which the bar can show at once, or none when they
 * differ, or a target keeps its own (`targets` being more). */
export function commonSeed(seeds: Map<LayerId, Gradient>, targets: number): Gradient | null {
  const [first, ...rest] = [...seeds.values()]
  if (!first || seeds.size !== targets) return null
  const same = (gradient: Gradient) => JSON.stringify(gradient) === JSON.stringify(first)
  return rest.every(same) ? first : null
}
