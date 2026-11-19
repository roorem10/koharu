'use client'

import { useMutation } from '@tanstack/react-query'

import { saveBooharuSection, useBooharuPreferences } from '@/lib/booharu/preferences'
import type { SfxMode } from '@koharu/ipc/protocol'

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
