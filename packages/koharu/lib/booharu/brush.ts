'use client'

// Booharu's brushes (S14-i). Koharu keeps one brush, a size and a colour, for Draw, the Eraser and
// Remove, and since Koharu 0.94.0 keeps it between runs with the tool (lib/store.ts persists
// 'koharu.workspace'). Booharu gives each of the three a brush of its own, as Photoshop does: its
// size, its hardness, and Draw's opacity, kept between runs under Booharu's own key,
// 'booharu.brushes', which holds those and nothing else; the tool and the colour, one for every
// tool, stay Koharu's. Picking a brush tool puts its size in Koharu's brush, which CanvasWorkspace
// sends the canvas as it sends any; a size set while one is picked, with the size field or slider,
// Alt+wheel, or [ and ], is that tool's. The hardnesses and Draw's opacity reach the canvas as the
// brush's feel (`booharu_set_brush`), which no settings file holds. Koharu's brush never gains
// either: persist's shallow merge would put an older stored brush, which lacks them, in its place.

import { useEffect } from 'react'
import { useHotkeys } from 'react-hotkeys-hook'
import { create } from 'zustand'
import { persist } from 'zustand/middleware'

import { controlKeepsKey, keyActions } from '@/lib/booharu/keys'
import { pageKey, queryClient, send } from '@/lib/queries'
import { maxBrushDiameter, MIN_BRUSH_DIAMETER, useKoharuStore, type CanvasTool } from '@/lib/store'
import { commands, type BrushFeel, type Page, type Size } from '@koharu/ipc/protocol'

/** The tools that paint with a brush, each with a brush of its own. */
export type BrushTool = 'draw' | 'eraser' | 'remove'

export function isBrushTool(tool: CanvasTool): tool is BrushTool {
  return tool === 'draw' || tool === 'eraser' || tool === 'remove'
}

/** A brush tool's own brush: its size, in page pixels, and its hardness, from 0, soft from the
 * stroke's centre line out, to 1, Koharu's hard edge. */
export interface BrushSlot {
  diameter: number
  hardness: number
}

/** Draw's brush, which has its opacity too, from just above 0 to 1: a stroke paints once at it,
 * never building up within itself, as Photoshop's Opacity. */
export interface DrawSlot extends BrushSlot {
  opacity: number
}

/** Each brush tool's brush. */
export interface BrushSlots {
  draw: DrawSlot
  eraser: BrushSlot
  remove: BrushSlot
}

const finite = (value: unknown): value is number =>
  typeof value === 'number' && Number.isFinite(value)

/** Whether `value` is a brush size: a number of at least 1 px. */
export const isDiameter = (value: unknown): value is number =>
  finite(value) && value >= MIN_BRUSH_DIAMETER

/** Whether `value` is a hardness: a number from 0 to 1. */
export const isHardness = (value: unknown): value is number =>
  finite(value) && value >= 0 && value <= 1

/** Whether `value` is an opacity: a number above 0 and at most 1. */
export const isOpacity = (value: unknown): value is number =>
  finite(value) && value > 0 && value <= 1

/** The size each brush starts at: Koharu's brush's, as lib/store.ts restored it, or its default,
 * 48, when it kept none, or none that's a size. */
function startingDiameter(): number {
  const { diameter } = useKoharuStore.getState().brush
  return isDiameter(diameter) ? diameter : useKoharuStore.getInitialState().brush.diameter
}

/** Each brush as it starts: at Koharu's size, hard and opaque, as Koharu's own brush is. */
export function startingSlots(): BrushSlots {
  const diameter = startingDiameter()
  return {
    draw: { diameter, hardness: 1, opacity: 1 },
    eraser: { diameter, hardness: 1 },
    remove: { diameter, hardness: 1 },
  }
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

/** The brushes `stored` holds, as 'booharu.brushes' kept them, each value taken only when it's a
 * number in its range, any other at its start: on a first load, with nothing stored, every brush
 * at Koharu's size, so the size Koharu kept stays when a brush tool is first picked. */
export function restoredSlots(stored: unknown): BrushSlots {
  const start = startingSlots()
  const kept = (tool: BrushTool): Record<string, unknown> => {
    const slot = isRecord(stored) ? stored[tool] : undefined
    return isRecord(slot) ? slot : {}
  }
  const take = (value: unknown, valid: (value: unknown) => value is number, otherwise: number) =>
    valid(value) ? value : otherwise
  const brush = (tool: BrushTool): BrushSlot => ({
    diameter: take(kept(tool).diameter, isDiameter, start[tool].diameter),
    hardness: take(kept(tool).hardness, isHardness, start[tool].hardness),
  })
  return {
    draw: { ...brush('draw'), opacity: take(kept('draw').opacity, isOpacity, start.draw.opacity) },
    eraser: brush('eraser'),
    remove: brush('remove'),
  }
}

/** Each brush tool's brush, the interface's own, kept between runs as Koharu keeps its brush. Its
 * store reads Koharu's, which hydrates as lib/store.ts creates it, at import, so Koharu's restored
 * size is there when this one hydrates: zustand merges a stored state, or none on a first load,
 * once, as the store is created. */
export const useBrushSlots = create<BrushSlots>()(
  persist(() => startingSlots(), {
    name: 'booharu.brushes',
    partialize: ({ draw, eraser, remove }) => ({ draw, eraser, remove }),
    merge: (stored) => restoredSlots(stored),
  }),
)

/** Sets `values` in `tool`'s brush, each only when it's a number in its range, and an opacity only
 * on Draw's, which alone has one. */
export function setBrushSlot(tool: BrushTool, values: Partial<DrawSlot>): void {
  const current: Partial<DrawSlot> = useBrushSlots.getState()[tool]
  const next = { ...current }
  if (isDiameter(values.diameter)) next.diameter = values.diameter
  if (isHardness(values.hardness)) next.hardness = values.hardness
  if (tool === 'draw' && isOpacity(values.opacity)) next.opacity = values.opacity
  const changed = (Object.keys(next) as (keyof DrawSlot)[]).some(
    (key) => next[key] !== current[key],
  )
  if (changed) useBrushSlots.setState({ [tool]: next } as Partial<BrushSlots>)
}

/** The brushes' feel, as the canvas takes it: each tool's hardness, and Draw's opacity. */
export function brushFeel({ draw, eraser, remove }: BrushSlots): BrushFeel {
  return {
    draw_hardness: draw.hardness,
    eraser_hardness: eraser.hardness,
    remove_hardness: remove.hardness,
    draw_opacity: draw.opacity,
  }
}

function sameFeel(a: BrushFeel, b: BrushFeel): boolean {
  return (
    a.draw_hardness === b.draw_hardness &&
    a.eraser_hardness === b.eraser_hardness &&
    a.remove_hardness === b.remove_hardness &&
    a.draw_opacity === b.draw_opacity
  )
}

/** The size one press of `[` (`-1`) or `]` (`1`) steps `diameter` to on a page of `page`'s size,
 * as Booharu 0.0.4's stepBrush did: by a tenth of the size, at least 2 px; never below 1 px, and
 * never past the largest size the page's size slider takes, though a size already past it, as
 * Alt+wheel can set, doesn't jump down on `]`. */
export function steppedDiameter(diameter: number, direction: -1 | 1, page: Size): number {
  const step = Math.max(2, Math.round(diameter * 0.1))
  return direction > 0
    ? Math.min(diameter + step, Math.max(diameter, maxBrushDiameter(page)))
    : Math.max(MIN_BRUSH_DIAMETER, diameter - step)
}

/** The page the canvas shows, which lib/events.ts keeps current in Koharu's read of it: none while
 * no project, or no page of it, shows. */
function shownPage(): Page | null {
  return queryClient.getQueryData<Page | null>(pageKey) ?? null
}

/** Steps the brush's size one press of `[` (`-1`) or `]` (`1`) on the page shown. */
function stepBrush(direction: -1 | 1): void {
  const page = shownPage()
  if (!page) return
  const { brush, setBrush } = useKoharuStore.getState()
  const diameter = steppedDiameter(brush.diameter, direction, page.size)
  if (diameter !== brush.diameter) setBrush({ ...brush, diameter })
}

/** The characters the two keys type, by default `[` and `]`. */
const smaller = keyActions.brushSmaller.key
const larger = keyActions.brushLarger.key

/** Whether AltGraph is on for `event`: a character typed with AltGr, as the desktop's page gets it
 * on Windows, Chromium's key map turning the Ctrl+Alt that typed it into AltGraph. */
function altGraph(event: KeyboardEvent): boolean {
  return typeof event.getModifierState === 'function' && event.getModifierState('AltGraph')
}

/** Whether a bracket key is left alone, unprevented, whichever way it matched: one another handler
 * took, or one pressed while no page shows, where a brush has no page to size it for. */
function leftAlone(event: KeyboardEvent): boolean {
  return event.defaultPrevented || shownPage() === null
}

/** Each brush tool's brush, mounted once, among the providers' hooks: a brush tool picked brings
 * its own size into Koharu's brush, a size set while it's picked is its own, and the brushes' feel
 * reaches the canvas as restored, then on each change. `[` and `]` step the size, as Photoshop's
 * do, with Brush, the Eraser or Remove, outside Koharu's Read mode, and on a page shown: matched by
 * the character typed, a bracket typed with AltGr included, in either shape it reaches the page in,
 * and by the key's place, the US keyboard's, for a layout that types no bracket there. A key a
 * focused control keeps, as the canvas's tool letters leave it, stays the control's. The hook calls
 * no query hook: Providers calls it outside the QueryClientProvider it renders. */
export function useBooharuBrush(): void {
  const tool = useKoharuStore((state) => state.tool)
  const reading = useKoharuStore((state) => state.reading)

  // A brush tool picked brings its size into Koharu's brush, keeping the colour, which every tool
  // shares, so CanvasWorkspace sends it to the canvas, as it sends any brush.
  useEffect(() => {
    if (!isBrushTool(tool)) return
    const { brush, setBrush } = useKoharuStore.getState()
    const { diameter } = useBrushSlots.getState()[tool]
    if (brush.diameter !== diameter) setBrush({ ...brush, diameter })
  }, [tool])

  // A size set while a brush tool is picked is that tool's: from the size field or slider,
  // Alt+wheel, which lib/events.ts puts in Koharu's store, or [ and ].
  useEffect(
    () =>
      useKoharuStore.subscribe(({ tool, brush }, previous) => {
        if (brush.diameter === previous.brush.diameter || !isBrushTool(tool)) return
        setBrushSlot(tool, { diameter: brush.diameter })
      }),
    [],
  )

  // The feel, as restored, then each time it changes; the desktop holds no other.
  useEffect(() => {
    let sent = brushFeel(useBrushSlots.getState())
    send(commands.booharuSetBrush, sent)
    return useBrushSlots.subscribe((slots) => {
      const feel = brushFeel(slots)
      if (sameFeel(feel, sent)) return
      sent = feel
      send(commands.booharuSetBrush, feel)
    })
  }, [])

  // As the tool letters' hotkey in CanvasWorkspace: a slider, a checkbox or another control that
  // takes no text lets a key through, while text entry, dialogs, popovers, menus and open lists
  // keep it. On only with a brush tool outside Read mode, a boolean, so no listener waits otherwise.
  const shared = {
    enabled: isBrushTool(tool) && !reading,
    enableOnFormTags: true,
    preventDefault: true,
  } as const
  // By the character typed, whatever the modifiers: AltGr+8, say, on a German keyboard, comes with
  // Ctrl and Alt on, as Windows reports AltGr, or with AltGraph alone, as Chromium's key map turns
  // them; react-hotkeys-hook would match neither shape without `ignoreModifiers`. Shift, Meta, and
  // Ctrl or Alt held alone are refused, and AltGraph counts as the chord it stands for, so a
  // slider keeps an AltGr bracket in either shape.
  useHotkeys([smaller, larger], (event) => stepBrush(event.key === smaller ? -1 : 1), {
    ...shared,
    useKey: true,
    ignoreModifiers: true,
    ignoreEventWhen: (event) =>
      leftAlone(event) ||
      event.shiftKey ||
      event.metaKey ||
      event.ctrlKey !== event.altKey ||
      controlKeepsKey(event, altGraph(event)),
  })
  // By the key's place, the brackets' on a US keyboard, for a layout that types something else
  // there: on Windows the desktop reads a German keyboard's Ü as ß and its + as a dead key, and a
  // browser reads the Ü as ü. A bracket typed is the hotkey's above, so one press steps once.
  useHotkeys(
    ['bracketleft', 'bracketright'],
    (event) => stepBrush(event.code === 'BracketLeft' ? -1 : 1),
    {
      ...shared,
      ignoreEventWhen: (event) =>
        event.key === smaller || event.key === larger || leftAlone(event) || controlKeepsKey(event),
    },
  )
}
