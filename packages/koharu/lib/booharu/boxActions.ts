'use client'

import { call } from '@/lib/backend'
import { openBoxDetails } from '@/lib/booharu/boxDetails'
import {
  findLayer,
  findPart,
  isBlockLayer,
  isLockedLayer,
  isTextLayer,
  type BlockLayer,
  type TextLayer,
} from '@/lib/document'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type Layer,
  type LayerId,
  type PageId,
  type SplitBalloon,
  type Stage,
} from '@koharu/ipc/protocol'

/** A text a box action acts on: a block, or a text layer; the layer holding it, null at the page's
 * top level; and whether it, or a layer holding it, is locked, which refuses every edit and run
 * that would change it. */
export interface Box {
  layer: BlockLayer | TextLayer
  parent: Layer | null
  locked: boolean
}

/** What a box action acts on: texts of one page. */
export interface BoxSelection {
  page: PageId
  boxes: Box[]
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

/** Whether a run may go over the selection's blocks again: it holds one, and none is locked, as a
 * run reaching a locked block stops. */
function rerunnable(selection: BoxSelection): boolean {
  const found = blocks(selection)
  return found.length > 0 && found.every(({ locked }) => !locked)
}

/** Runs `stage` on the selection's blocks alone, as Story's own Translate runs one block. */
function rerun(selection: BoxSelection, stage: Stage): Promise<unknown> {
  const ids = blocks(selection).map(({ layer }) => layer.id)
  return call(commands.runPipeline, { blocks: ids }, [stage])
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
    run: (selection) => rerun(selection, 'recognize'),
  },
  {
    id: 'translate',
    label: 'booharu.boxes.translate',
    enabled: rerunnable,
    run: (selection) => rerun(selection, 'translate'),
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
