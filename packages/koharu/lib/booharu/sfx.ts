'use client'

import { useMutation } from '@tanstack/react-query'
import { useCallback, useRef, useState } from 'react'

import { saveBooharuSection, useBooharuPreferences } from '@/lib/booharu/preferences'
import type { SfxMode, SubtitleStyle } from '@koharu/ipc/protocol'

/** Each SFX mode, in Settings' order: a record over every mode, so one the desktop adds fails to
 * compile until it's placed. */
const modes = {
  skip: true,
  inside_bubbles: true,
  translate_all: true,
} as const satisfies Record<SfxMode, true>

/** The SFX modes, in the order Settings lists them. */
export const sfxModes = Object.keys(modes) as SfxMode[]

/** `[booharu-lettering] sfx` while the file sets none, as booharu-core's default: sound effects
 * stay as the original art. */
export const DEFAULT_SFX_MODE: SfxMode = 'skip'

/** Which sound effects a run translates, `sfx` of `[booharu-lettering]`, and `set`, which saves a
 * choice at once over what the file holds just before. The choice being saved shows until the save
 * ends, then what the file holds, so after a failed save, whose error shows as every failed
 * command's does, the file's choice shows again. `mode` is undefined until the first read ends. */
export function useSfxMode() {
  const preferences = useBooharuPreferences()
  const save = useMutation({
    mutationFn: (sfx: SfxMode) =>
      saveBooharuSection('booharu-lettering', (section) => ({ ...section, sfx })),
  })
  const section = preferences.data?.['booharu-lettering']
  const saved = section ? (section.sfx ?? DEFAULT_SFX_MODE) : undefined
  return {
    mode: save.isPending ? save.variables : saved,
    set: (sfx: SfxMode) => save.mutate(sfx),
  }
}

/** A sound effect subtitle's look with every value, as the desktop answers it. */
export type SubtitleLook = Required<SubtitleStyle>

/** The subtitle look while the file sets none, as booharu-core's defaults: the default font, 1.5%
 * of the page's shorter side, black text with a white outline. */
export const builtInSubtitle: SubtitleLook = {
  family: null,
  size: 0.015,
  fill: [0, 0, 0, 1],
  stroke: [1, 1, 1, 1],
}

/** The percentages of the page's shorter side Settings takes for a subtitle's size, as booharu-core
 * takes a share above 0 and at most 0.1. */
export const SUBTITLE_PERCENT = { min: 0.1, max: 10, step: 0.1 } as const

/** A subtitle's `size`, a share of the page's shorter side, as Settings shows it: a percentage, to
 * two decimals. */
export function subtitlePercent(size: number): number {
  return Math.round(size * 10_000) / 100
}

/** The share of the page's shorter side `percent` saves as a subtitle's `size`; null for a
 * percentage Settings doesn't take: none, or one outside 0.1 to 10. */
export function subtitleShare(percent: number | null): number | null {
  if (percent === null || !Number.isFinite(percent)) return null
  if (percent < SUBTITLE_PERCENT.min || percent > SUBTITLE_PERCENT.max) return null
  return Math.round(percent * 100) / 10_000
}

/** The sound effect subtitles' look, `subtitle` of `[booharu-lettering]`, with the changes being
 * saved over it, and `change`, which saves one at once, made to what the file holds just before,
 * replacing only the keys it names, so the rest of the section, and a key another writer changed
 * meanwhile, stay as the file has them. Once the last save ends, what the file holds shows again,
 * so after a failed save, whose error shows as every failed command's does, the file's look shows.
 * `look` is undefined until the first read ends. */
export function useSubtitleLook() {
  const preferences = useBooharuPreferences()
  const [unsaved, setUnsaved] = useState<Partial<SubtitleLook> | null>(null)
  /** Counts changes, so a save that ends knows whether a later change is still being saved. */
  const changes = useRef(0)

  const change = useCallback((patch: Partial<SubtitleLook>) => {
    changes.current += 1
    const last = changes.current
    setUnsaved((shown) => ({ ...shown, ...patch }))
    void saveBooharuSection('booharu-lettering', (section) => ({
      ...section,
      subtitle: { ...section.subtitle, ...patch },
    }))
      .catch(() => undefined)
      .finally(() => {
        if (changes.current === last) setUnsaved(null)
      })
  }, [])

  const section = preferences.data?.['booharu-lettering']
  const look: SubtitleLook | undefined = section
    ? { ...builtInSubtitle, ...section.subtitle, ...unsaved }
    : undefined
  return { look, change }
}
