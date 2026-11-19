'use client'

import { call } from '@/lib/backend'
import { openBoxDetails, showLayer } from '@/lib/booharu/boxDetails'
import { fetchLettering } from '@/lib/booharu/boxLettering'
import { openExplanation } from '@/lib/booharu/explain'
import { redoBoxes } from '@/lib/booharu/redo'
import {
  findLayer,
  findPart,
  isBlockLayer,
  isLockedLayer,
  isTextLayer,
  plainText,
  type BlockLayer,
  type TextLayer,
} from '@/lib/document'
import i18n from '@/lib/i18n'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type BlockLettering,
  type Layer,
  type LayerId,
  type PageId,
  type ResetCounts,
  type SplitBalloon,
  type Stage,
} from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

/** A text a box action acts on: a block, or a text layer; the layer holding it, null at the page's
 * top level; and whether it, or a layer holding it, is locked, which refuses every edit and run
 * that would change it. */
export interface Box {
  layer: BlockLayer | TextLayer
  parent: Layer | null
  locked: boolean
}

/** What a box action acts on: texts of one page, with the page's lettering, by block, as the views
 * last read it through `useLettering`; undefined while it isn't known, which leaves an action that
 * needs a block's category, Add subtitle, off. */
export interface BoxSelection {
  page: PageId
  boxes: Box[]
  lettering?: ReadonlyMap<LayerId, BlockLettering>
}

/** The texts `selected` names among `layers`, a page's, in its order and each once: blocks and
 * text layers, a block's selected mask or clean-up standing for the block. Other layers are left
 * out. */
export function selectedBoxes(layers: Layer[], selected: LayerId[]): Box[] {
  const ids = new Set(selected.map((id) => findPart(layers, id)?.block.id ?? id))
  return [...ids].flatMap((id) => {
    const placed = findLayer(layers, id)
    if (!placed) return []
    const { layer, ancestors } = placed
    if (!isBlockLayer(layer) && !isTextLayer(layer)) return []
    return [{ layer, parent: ancestors.at(-1) ?? null, locked: isLockedLayer(placed) }]
  })
}

/** The texts a balloon holds, in tree order, which is reading order, its sound effects and hidden
 * blocks left out: the blocks a join makes one, as the desktop's join takes them. */
export function balloonTexts(balloon: Layer): BlockLayer[] {
  return (balloon.children ?? []).filter(
    (child): child is BlockLayer => isBlockLayer(child) && child.role !== 'sfx' && child.visible,
  )
}

/** One per-box action, listed once for the Story row menu, the Edit menu and Box details alike:
 * its name, when it can act on a selection, and what it does. */
export interface BoxAction {
  id: string
  /** Its name, an interface string's key. */
  label: string
  enabled: (selection: BoxSelection) => boolean
  /** Acts on a selection it's enabled for. A failed command shows its error, as every command's
   * does, and rejects. */
  run: (selection: BoxSelection) => Promise<unknown>
}

/** The one text a selection holds, or none. */
function only(selection: BoxSelection): Box | null {
  return selection.boxes.length === 1 ? (selection.boxes[0] ?? null) : null
}

/** The selection's blocks, which runs read, translate and clean; a text layer has nothing to. */
function blocks(selection: BoxSelection): Box[] {
  return selection.boxes.filter(({ layer }) => isBlockLayer(layer))
}

/** The ids of the selection's blocks. */
function blockIds(selection: BoxSelection): LayerId[] {
  return blocks(selection).map(({ layer }) => layer.id)
}

/** Whether a run may go over the selection's blocks again: one of them isn't locked, as a run
 * leaves the locked ones alone and counts them. */
function rerunnable(selection: BoxSelection): boolean {
  return blocks(selection).some(({ locked }) => !locked)
}

/** Runs `stage` on the selection's blocks alone, asking nothing, as Clean again does. */
function rerun(selection: BoxSelection, stage: Stage): Promise<unknown> {
  return call(commands.runPipeline, { blocks: blockIds(selection) }, [stage])
}

/** The texts a join of `balloon` would make one, in reading order, the first kept; none unless it
 * holds two or more and nothing the join changes is locked, which would refuse it all. `locked` is
 * whether the balloon, or a layer holding it, is locked. */
function joinable(balloon: Layer, locked: boolean): BlockLayer[] | null {
  const texts = balloonTexts(balloon)
  const kept = texts[0]
  if (locked || !kept || texts.length < 2 || texts.some(({ locked }) => locked)) return null
  // The kept block's mask and clean-up take the others' pixels.
  return kept.mask?.locked || kept.cleanup?.locked ? null : texts
}

/** The texts a join of the selection's one text block would make one, its balloon's, as
 * `joinable` finds them; none unless the block shows, as a hidden one is no part of the join. */
function joining(selection: BoxSelection): BlockLayer[] | null {
  const box = only(selection)
  if (!box || box.locked || !isBlockLayer(box.layer) || box.layer.role === 'sfx') return null
  if (!box.layer.visible) return null
  // The box isn't locked, so neither is its balloon nor a layer holding that.
  return box.parent?.kind === 'balloon' ? joinable(box.parent, false) : null
}

/** Whether a join of every balloon the desktop found split on a page, `split`, among `layers`, the
 * page's, can go ahead: there's one, and nothing any of them would change is locked, as a lock in
 * one refuses them all. */
export function joinsPage(layers: Layer[], split: SplitBalloon[]): boolean {
  return (
    split.length > 0 &&
    split.every(({ balloon }) => {
      const placed = findLayer(layers, balloon)
      return placed !== undefined && joinable(placed.layer, isLockedLayer(placed)) !== null
    })
  )
}

/** The selection's one block a split cuts in two, a sound effect's as well as a text's, as the
 * desktop splits either, or none: it and its parts unlocked, its source in a frame to cut. */
function splitting(selection: BoxSelection): BlockLayer | null {
  const box = only(selection)
  if (!box || box.locked || !isBlockLayer(box.layer)) return null
  const { layer } = box
  const framed = typeof layer.source.frame === 'object'
  const free = !layer.mask?.locked && !layer.cleanup?.locked
  return framed && free ? layer : null
}

/** The selection's one block whose category can change, or none: it isn't locked, as the scene
 * refuses a label on a locked block. A text layer has no category. */
function categorized(selection: BoxSelection): BlockLayer | null {
  const box = only(selection)
  return box && !box.locked && isBlockLayer(box.layer) ? box.layer : null
}

/** The selection's one sound effect a subtitle can go beside, or none: a block whose category, as
 * its page's lettering says, is `sfx`, which the desktop's rule for a sound effect follows, and
 * that isn't locked, as the subtitle's edit also hides the effect's translation and clean-up. One
 * with a subtitle already counts: Add subtitle then selects that one. */
function soundEffect(selection: BoxSelection): BlockLayer | null {
  const box = only(selection)
  if (!box || box.locked || !isBlockLayer(box.layer)) return null
  return selection.lettering?.get(box.layer.id)?.category === 'sfx' ? box.layer : null
}

/** Whether Reset to lettering default gives `box` a look back: a translated block or a text layer,
 * neither locked; an untranslated block has no lettering. */
function resettable({ layer, locked }: Box): boolean {
  return !locked && (isTextLayer(layer) || (isBlockLayer(layer) && layer.translation != null))
}

/** The toast after Reset to lettering default, `counts` its answer: how many texts it reset, or
 * that it reset none, with how many it left alone as locked or untranslated; with none reset and
 * none left alone, every text had its default already. */
function resetNotice(counts: ResetCounts): void {
  const notes = [
    counts.locked > 0 ? i18n.t('booharu.boxes.resetLocked', { count: counts.locked }) : null,
    counts.untranslated > 0
      ? i18n.t('booharu.boxes.resetUntranslated', { count: counts.untranslated })
      : null,
  ].filter((note): note is string => note !== null)
  const description = notes.join(' ') || undefined
  if (counts.changed > 0) {
    toast.add({
      type: 'success',
      title: i18n.t('booharu.boxes.resetDone', { count: counts.changed }),
      description,
    })
  } else {
    const title = i18n.t(description ? 'booharu.boxes.resetNone' : 'booharu.boxes.resetAlready')
    toast.add({ type: 'info', title, description })
  }
}

/** The selection's one block whose line Explain this line can explain, or none: it has a source to
 * explain. A locked one can be explained, as explaining changes nothing. */
function explainable(selection: BoxSelection): BlockLayer | null {
  const box = only(selection)
  return box && isBlockLayer(box.layer) && plainText(box.layer.source).trim() ? box.layer : null
}

/** After a join, as Story after a delete: a joined text that went, or its selected part, leaves the
 * selection, and the text it joined into takes its place. `joins` are each join's texts in reading
 * order, the first kept. */
function selectJoined(joins: BlockLayer[][]): void {
  const into = new Map<LayerId, LayerId>()
  for (const [kept, ...gone] of joins) {
    if (!kept) continue
    for (const block of gone) {
      for (const id of [block.id, block.mask?.id, block.cleanup?.id]) {
        if (id) into.set(id, kept.id)
      }
    }
  }
  const { selectedLayers, selectLayers } = useKoharuStore.getState()
  if (selectedLayers.some((id) => into.has(id))) {
    selectLayers(selectedLayers.map((id) => into.get(id) ?? id))
  }
}

/** Every per-box action, in the order the menus list them. A later one goes in as one more entry. */
export const boxActions: readonly BoxAction[] = [
  {
    id: 'read',
    label: 'booharu.boxes.read',
    enabled: rerunnable,
    // Asks first when a source was corrected by hand.
    run: (selection) => redoBoxes(blockIds(selection), 'recognize'),
  },
  {
    id: 'translate',
    label: 'booharu.boxes.translate',
    enabled: rerunnable,
    // Asks first when a translation was written or fixed by hand.
    run: (selection) => redoBoxes(blockIds(selection), 'translate'),
  },
  {
    id: 'clean',
    label: 'booharu.boxes.clean',
    enabled: rerunnable,
    run: (selection) => rerun(selection, 'inpaint'),
  },
  {
    id: 'join',
    label: 'booharu.boxes.join',
    enabled: (selection) => joining(selection) !== null,
    run: async (selection) => {
      const texts = joining(selection)
      const box = only(selection)
      if (!texts || !box) return
      await call(commands.booharuJoinBalloon, box.layer.id)
      selectJoined([texts])
    },
  },
  {
    id: 'split',
    label: 'booharu.boxes.split',
    enabled: (selection) => splitting(selection) !== null,
    run: async (selection) => {
      const block = splitting(selection)
      if (block) await call(commands.booharuSplitBlock, block.id)
    },
  },
  {
    id: 'subtitle',
    label: 'booharu.boxes.subtitle',
    enabled: (selection) => soundEffect(selection) !== null,
    run: async (selection) => {
      const block = soundEffect(selection)
      if (!block) return
      // From the effect's translation; with none, a Translate run makes the subtitle as it writes.
      const { layer, run } = await call(commands.booharuAddSfxSubtitle, block.id, null)
      if (layer !== null) await showLayer(selection.page, layer)
      else if (run !== null) {
        toast.add({ type: 'info', title: i18n.t('booharu.boxes.subtitleTranslating') })
      }
    },
  },
  {
    id: 'category',
    label: 'booharu.lettering.change',
    enabled: (selection) => categorized(selection) !== null,
    run: async (selection) => {
      const block = categorized(selection)
      if (!block) return
      // Read first, so Box details opens with the section's picker there to take the focus; a read
      // that fails shows its error, as a command's does, and opens nothing.
      await fetchLettering(selection.page)
      openBoxDetails(selection.page, block.id, 'lettering')
    },
  },
  {
    id: 'reset',
    label: 'booharu.boxes.reset',
    enabled: (selection) => selection.boxes.some(resettable),
    // Every selected text goes, so the toast counts the locked and untranslated ones it leaves.
    run: async (selection) => {
      if (!selection.boxes.some(resettable)) return
      const layers = selection.boxes.map(({ layer }) => layer.id)
      resetNotice(await call(commands.booharuResetLettering, layers))
    },
  },
  {
    id: 'explain',
    label: 'booharu.context.explain',
    enabled: (selection) => explainable(selection) !== null,
    run: async (selection) => {
      const block = explainable(selection)
      if (block) await openExplanation(selection.page, block)
    },
  },
  {
    id: 'details',
    label: 'booharu.details.title',
    enabled: (selection) => only(selection) !== null,
    run: async (selection) => {
      const box = only(selection)
      if (box) openBoxDetails(selection.page, box.layer.id)
    },
  },
]

/** Runs `action` on `selection`, as a menu item or a button does; a failed command has shown its
 * error. */
export function runBoxAction(action: BoxAction, selection: BoxSelection): void {
  void action.run(selection).catch(() => undefined)
}

/** Joins every balloon of `page` whose text is split, `split` as the desktop found them among
 * `layers`, the page's, as one undo step; the selection follows the joined texts. */
export async function joinPage(
  page: PageId,
  layers: Layer[],
  split: SplitBalloon[],
): Promise<void> {
  await call(commands.booharuJoinPage, page)
  selectJoined(
    split.map(({ blocks: ids }) =>
      ids.flatMap((id) => {
        const layer = findLayer(layers, id)?.layer
        return layer && isBlockLayer(layer) ? [layer] : []
      }),
    ),
  )
}
