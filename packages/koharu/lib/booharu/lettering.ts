'use client'

import { useMutation } from '@tanstack/react-query'
import { useCallback, useEffect, useRef, useState } from 'react'

import {
  saveBooharuSection,
  useBooharuPreferences,
  writeBooharuSection,
} from '@/lib/booharu/preferences'
import type { DefaultTextStyle } from '@koharu/ipc/protocol'

/** The id of Settings' default text style section, which the colour detection switch links to. */
export const defaultTextStyleSection = 'booharu-default-text-style'

/** `[booharu-lettering]`'s default text style with every value, as the desktop answers it. */
export type DefaultLook = Required<DefaultTextStyle>

/** The default text style when the section sets nothing: Koharu's black text with no outline, and
 * a 1.5 px width ready for one, as booharu-core's defaults are. */
export const builtInLook: DefaultLook = { fill: null, stroke: null, stroke_width: 1.5 }

/** The widest outline the default text style takes, in pixels, as booharu-core checks it. */
export const MAX_OUTLINE_WIDTH = 50

/** How long a change waits for the next before it's saved, as Koharu's settings autosave waits. */
const SAVE_DELAY = 260

/** The `flush` of each default text style section showing, which saves its change still waiting. */
const flushes = new Set<() => void>()

/** Saves the default text style's change still waiting at once, as Apply does before it counts or
 * applies, in the section's own row or the Lettering categories card's. */
export function flushDefaultTextStyle() {
  for (const flush of flushes) flush()
}

/** Whether `width` is an outline width the section takes: finite and from 0 to 50. */
export function isOutlineWidth(width: number | null): width is number {
  return width !== null && Number.isFinite(width) && width >= 0 && width <= MAX_OUTLINE_WIDTH
}

/** Whether `look` is the built-in one, so there's nothing to clear. */
export function isBuiltIn(look: DefaultLook): boolean {
  return (
    look.fill === null && look.stroke === null && look.stroke_width === builtInLook.stroke_width
  )
}

/** The default text style the settings file holds, with the changes not yet saved over it, and
 * `change`, which takes one: changes wait until none has come for a moment, as Koharu's settings
 * do, then are saved together over what the file holds just before, replacing only the keys they
 * name, so a key or section another writer changed meanwhile stays as that writer left it. A
 * change still waiting when the section goes is saved then, and `flushDefaultTextStyle` saves one
 * at once, as Apply does before it counts. Once the last save ends, what the file holds shows
 * again, so after a failed save, whose error shows as every failed command's does, the section
 * shows the file's values. */
export function useDefaultTextStyle() {
  const preferences = useBooharuPreferences()
  const [unsaved, setUnsaved] = useState<Partial<DefaultLook> | null>(null)
  const waiting = useRef<{ patch: Partial<DefaultLook>; timer: number } | null>(null)
  /** Counts changes, so a save that ends knows whether a later change is still to come. */
  const changes = useRef(0)

  const flush = useCallback(() => {
    const queued = waiting.current
    if (!queued) return
    window.clearTimeout(queued.timer)
    waiting.current = null
    const last = changes.current
    void saveBooharuSection('booharu-lettering', (section) => ({
      ...section,
      default: { ...section.default, ...queued.patch },
    }))
      .catch(() => undefined)
      .finally(() => {
        if (changes.current === last) setUnsaved(null)
      })
  }, [])

  // A change still waiting is saved when Apply asks, wherever its row is, and when the section
  // goes, such as when another tab opens.
  useEffect(() => {
    flushes.add(flush)
    return () => {
      flushes.delete(flush)
      flush()
    }
  }, [flush])

  const change = useCallback(
    (patch: Partial<DefaultLook>) => {
      changes.current += 1
      setUnsaved((shown) => ({ ...shown, ...patch }))
      const queued = waiting.current
      if (queued) window.clearTimeout(queued.timer)
      waiting.current = {
        patch: { ...queued?.patch, ...patch },
        timer: window.setTimeout(flush, SAVE_DELAY),
      }
    },
    [flush],
  )

  const saved = preferences.data?.['booharu-lettering'].default
  const look: DefaultLook = {
    fill: saved?.fill ?? builtInLook.fill,
    stroke: saved?.stroke ?? builtInLook.stroke,
    stroke_width: saved?.stroke_width ?? builtInLook.stroke_width,
    ...unsaved,
  }
  return { look, loaded: preferences.data !== undefined, change }
}

/** Whether new translations keep the colours of the original lettering, `detect_colors` of
 * `[booharu-lettering]`, and `set`, which saves a choice at once over what the file holds just
 * before. The choice being saved shows until the save ends, then what the file holds, so after a
 * failed save, whose error shows as every failed command's does, the file's choice shows again. */
export function useDetectColors() {
  const preferences = useBooharuPreferences()
  const save = useMutation({
    mutationFn: (detect: boolean) =>
      writeBooharuSection('booharu-lettering', (section) => ({
        ...section,
        detect_colors: detect,
      })),
  })
  const saved = preferences.data?.['booharu-lettering'].detect_colors ?? true
  return {
    detect: save.isPending ? save.variables : saved,
    loaded: preferences.data !== undefined,
    set: (detect: boolean) => save.mutate(detect),
  }
}
