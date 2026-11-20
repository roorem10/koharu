'use client'

import { useCallback, useRef, useState } from 'react'

import {
  booharuPreferencesKey,
  queueBooharuWrite,
  saveBooharuSection,
  useBooharuPreferences,
} from '@/lib/booharu/preferences'
import { queryClient } from '@/lib/queries'
import {
  commands,
  type BooharuPreferences,
  type EmphasisSettings,
  type EmphasisStyle,
  type LetteringCategory,
} from '@koharu/ipc/protocol'

/** Which emphasis Settings shows: the default, or a lettering category's own. */
export type EmphasisChoice = 'default' | LetteringCategory

/** An emphasis style with every value, each null for none, as the desktop answers it. */
export type EmphasisLook = Required<EmphasisStyle>

/** One of an emphasis style's values, by its key. */
export type EmphasisKey = keyof EmphasisLook

/** A style setting no value: what a category with no entry of its own sets, so its stressed words
 * look as the default's do. */
export const NO_EMPHASIS: EmphasisLook = {
  family: null,
  weight: null,
  italic: null,
  size_factor: null,
  fill: null,
  stroke: null,
  stroke_width: null,
  outer_stroke: null,
  outer_stroke_width: null,
}

/** `[booharu-emphasis]` while the file sets nothing, as booharu-core's default: the AI stresses
 * words, which are bold, at 115% of their text's size. */
export const BUILT_IN_EMPHASIS = {
  ai: true,
  default: { ...NO_EMPHASIS, weight: 700, size_factor: 1.15 },
  categories: {},
} satisfies Required<EmphasisSettings>

/** The values `choice` sets itself: the default's, or a category's own, every one none for a
 * category with no entry. */
export function ownLook(settings: EmphasisSettings, choice: EmphasisChoice): EmphasisLook {
  const own =
    choice === 'default'
      ? (settings.default ?? BUILT_IN_EMPHASIS.default)
      : settings.categories?.[choice]
  return { ...NO_EMPHASIS, ...own }
}

/** Whether `style` sets no value, which the file takes for neither the default nor a category. */
export function setsNothing(style: EmphasisStyle): boolean {
  return Object.values(style).every((value) => value === null || value === undefined)
}

/** A change to `[booharu-emphasis]`, made to it as the file holds it just before the save. */
export type EmphasisEdit = (settings: EmphasisSettings) => EmphasisSettings

type EmphasisCategories = NonNullable<EmphasisSettings['categories']>

/** `categories` with `category`'s entry `look`, or without it for `look` null; an entry setting
 * nothing goes, as the file takes none. */
function withEntry(
  categories: EmphasisCategories,
  category: LetteringCategory,
  look: EmphasisLook | null,
): EmphasisCategories {
  const others = { ...categories }
  delete others[category]
  return look && !setsNothing(look) ? { ...others, [category]: look } : others
}

/** Turns the AI's emphasis on or off. */
export function setAi(ai: boolean): EmphasisEdit {
  return (settings) => ({ ...settings, ai })
}

/** Gives `choice` the value `value` for `key`, or none for null, so it takes the text's own value,
 * for the default, or the default's, for a category; a category left setting nothing loses its
 * entry. The default left setting nothing is refused by the desktop, as the file takes none. */
export function setEmphasis<Key extends EmphasisKey>(
  choice: EmphasisChoice,
  key: Key,
  value: EmphasisLook[Key],
): EmphasisEdit {
  return (settings) => {
    const look = { ...ownLook(settings, choice), [key]: value }
    if (choice === 'default') return { ...settings, default: look }
    return { ...settings, categories: withEntry(settings.categories ?? {}, choice, look) }
  }
}

/** Takes `category`'s entry away, so its stressed words look as the default's do. */
export function resetCategory(category: LetteringCategory): EmphasisEdit {
  return (settings) => ({
    ...settings,
    categories: withEntry(settings.categories ?? {}, category, null),
  })
}

/** The weights Settings takes for emphasis, as booharu-core takes them: its arrows step by 100. */
export const EMPHASIS_WEIGHT = { min: 1, max: 1000, step: 100 } as const

/** The sizes Settings takes for emphasis, as percentages of the text's: booharu-core's size factors
 * from 0.25 to 4. */
export const EMPHASIS_PERCENT = { min: 25, max: 400, step: 1 } as const

/** The outline widths Settings takes for emphasis, in pixels, as booharu-core takes them. */
export const EMPHASIS_WIDTH = { min: 0, max: 100, step: 0.5 } as const

/** A size factor as Settings shows it: a percentage of the text's size, to two decimals. */
export function emphasisPercent(factor: number): number {
  return Math.round(factor * 10_000) / 100
}

/** The size factor `percent` saves; null for a percentage Settings doesn't take: one outside 25 to
 * 400, or no number. */
export function emphasisFactor(percent: number): number | null {
  if (!Number.isFinite(percent)) return null
  if (percent < EMPHASIS_PERCENT.min || percent > EMPHASIS_PERCENT.max) return null
  return Math.round(percent * 100) / 10_000
}

/** The weight `weight` saves, a whole one; null for one outside 1 to 1000, or no number. */
export function emphasisWeight(weight: number): number | null {
  const whole = Math.round(weight)
  if (!Number.isFinite(whole)) return null
  return whole >= EMPHASIS_WEIGHT.min && whole <= EMPHASIS_WEIGHT.max ? whole : null
}

/** The outline width `width` saves, to two decimals; null for one outside 0 to 100 pixels, or no
 * number. */
export function emphasisWidth(width: number): number | null {
  if (!Number.isFinite(width)) return null
  if (width < EMPHASIS_WIDTH.min || width > EMPHASIS_WIDTH.max) return null
  return Math.round(width * 100) / 100
}

/** `[booharu-emphasis]` as the last read or save left it in the shared settings. */
function read(): EmphasisSettings {
  const preferences = queryClient.getQueryData<BooharuPreferences>(booharuPreferencesKey)
  return preferences?.['booharu-emphasis'] ?? BUILT_IN_EMPHASIS
}

/** Reads the Booharu settings again once the writes queued before end, so the views showing them
 * show what the file holds. A read that fails leaves them as they are, without a second error, as
 * the failed save has shown one. */
function reload(): Promise<void> {
  return queueBooharuWrite(async () => {
    const current = await commands.booharuGetPreferences().catch(() => null)
    if (current) queryClient.setQueryData(booharuPreferencesKey, current)
  })
}

/** `[booharu-emphasis]` as the settings file holds it, read again each time a view showing it
 * opens, with the changes being saved over it, and `change`, which saves one at once, made to what
 * the file holds just before, so a value another writer, such as the agent's settings tool,
 * changed meanwhile stays as the file has it; it answers whether it saved. Saves run one after
 * another, as 0.0.4's did, and after a failed one, whose error shows as every failed command's
 * does, the section is read again, as 0.0.4 read it, so once the last change ends what the file
 * holds shows. `settings` is undefined until the first read ends. */
export function useEmphasisSettings() {
  const preferences = useBooharuPreferences()
  const [unsaved, setUnsaved] = useState<EmphasisSettings | null>(null)
  /** Counts changes, so a save that ends knows whether a later change is still being saved. */
  const changes = useRef(0)

  const change = useCallback((edit: EmphasisEdit): Promise<boolean> => {
    changes.current += 1
    const last = changes.current
    setUnsaved((shown) => edit(shown ?? read()))
    return saveBooharuSection('booharu-emphasis', edit)
      .then(
        () => true,
        () => {
          // A later change's save reads the file before it writes anyway.
          if (changes.current === last) void reload()
          return false
        },
      )
      .finally(() => {
        if (changes.current === last) setUnsaved(null)
      })
  }, [])

  return { settings: unsaved ?? preferences.data?.['booharu-emphasis'], change }
}
