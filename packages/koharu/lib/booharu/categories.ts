'use client'

import { useCallback, useRef, useState } from 'react'

import {
  booharuPreferencesKey,
  saveBooharuSection,
  useBooharuPreferences,
} from '@/lib/booharu/preferences'
import { queryClient } from '@/lib/queries'
import type {
  BooharuPreferences,
  BubbleShape,
  CategoryFont,
  FontFamily,
  LetteringCategory,
  LetteringSettings,
} from '@koharu/ipc/protocol'

/** The id of Settings' Lettering categories card, which a link can take the focus to. */
export const letteringCategoriesSection = 'booharu-lettering-categories'

/** The bubble each category also letters, for the four booharu-core's `shape_category` gives a
 * balloon's shape, `none` being text outside any bubble; a record over every category in Settings'
 * order, `LetteringCategory::ALL`'s, so one the desktop adds fails to compile until it's placed. */
const shapes = {
  normal_dialogue: 'round',
  thoughts: null,
  floating: 'none',
  small_text: null,
  narration_box: 'rectangle',
  shout: 'spiky',
  weak_shout: null,
  sfx: null,
  moan: null,
  title: null,
  phone_text: null,
  whisper: null,
  shy: null,
  crying: null,
  menacing: null,
  handwritten: null,
  sign: null,
  electronic_voice: null,
} as const satisfies Record<LetteringCategory, BubbleShape | 'none' | null>

/** Every lettering category, in Settings' order. */
export const letteringCategories = Object.keys(shapes) as LetteringCategory[]

/** The string key of `category`'s name. */
export function categoryName(category: LetteringCategory) {
  return `booharu.lettering.categories.${category}` as const
}

/** The string key of the note saying which bubbles `category` letters too, for the four a bubble's
 * shape gives; null for the others. */
export function shapeNote(category: LetteringCategory) {
  const shape = shapes[category]
  return shape ? (`booharu.lettering.shapeNotes.${shape}` as const) : null
}

/** The categories given their own font, by id, as `[booharu-lettering] categories` holds them. */
export type CategoryFonts = NonNullable<LetteringSettings['categories']>

/** What the Lettering categories card shows and changes of `[booharu-lettering]`: Pick fonts
 * automatically and the categories' own fonts. */
export type CategorySettings = { auto: boolean; categories: CategoryFonts }

/** A change to the card's part of the section, made to it as the file holds it just before the
 * save. */
export type CategoryEdit = (settings: CategorySettings) => CategorySettings

/** A font as a category letters in it: its family, null for a copy of the default font, and
 * whether it's bold and italic. */
export type LetteredFont = { family: string | null; bold: boolean; italic: boolean }

const NORMAL: LetteringCategory = 'normal_dialogue'

/** The font `category` is lettered in, as booharu-core's `category_font` picks it: its own entry,
 * else Normal Dialogue's, an entry without a family taking Normal Dialogue's; else none, which
 * leaves the default font, neither bold nor italic. */
export function letteredFont(categories: CategoryFonts, category: LetteringCategory): LetteredFont {
  const entry = categories[category] ?? categories[NORMAL]
  return {
    family: entry?.family ?? categories[NORMAL]?.family ?? null,
    bold: entry?.bold ?? false,
    italic: entry?.italic ?? false,
  }
}

/** The family `category`'s font picker shows: its own, or an empty name for none of its own,
 * which the picker's leading row stands for, "Same as Normal Dialogue", or for Normal Dialogue,
 * "Same as the default font". */
export function shownFamily(categories: CategoryFonts, category: LetteringCategory): string {
  return categories[category]?.family ?? ''
}

/** `categories` with `category`'s entry, or without it for `font` null. An entry holds one of its
 * three at least, as the section takes no other: one with none is left out. */
function withEntry(
  categories: CategoryFonts,
  category: LetteringCategory,
  font: LetteredFont | null,
): CategoryFonts {
  const others = { ...categories }
  delete others[category]
  return font && (font.family || font.bold || font.italic)
    ? { ...others, [category]: font satisfies CategoryFont }
    : others
}

/** Turns Pick fonts automatically on or off. */
export function setAuto(auto: boolean): CategoryEdit {
  return (settings) => ({ ...settings, auto })
}

/** Letters `category` in `family`, as bold and italic as it's lettered now; or with an empty
 * `family`, the picker's leading row, takes away only its own family, keeping its own bold and
 * italic, so it's lettered in Normal Dialogue's family, or Normal Dialogue in the default font; an
 * entry left setting nothing goes, and a category without one is lettered so already. */
export function setFamily(category: LetteringCategory, family: string): CategoryEdit {
  return (settings) => {
    const own = settings.categories[category]
    if (!family && !own) return settings
    const { bold, italic } = letteredFont(settings.categories, category)
    const font = { family: family || null, bold, italic }
    return { ...settings, categories: withEntry(settings.categories, category, font) }
  }
}

/** Turns `category`'s bold or italic, `style`, on or off, keeping the other as it's lettered now,
 * Normal Dialogue's when it had no entry, and its own family, or none, so one without it keeps
 * following Normal Dialogue's. */
export function setStyle(
  category: LetteringCategory,
  style: 'bold' | 'italic',
  on: boolean,
): CategoryEdit {
  return (settings) => {
    const { bold, italic } = letteredFont(settings.categories, category)
    const family = settings.categories[category]?.family ?? null
    const next = { family, bold, italic, [style]: on }
    return { ...settings, categories: withEntry(settings.categories, category, next) }
  }
}

/** 0.0.4's starting fonts: each category's candidates, the first available one winning, and whether
 * it's italic. A category not listed, as the three 0.0.5 adds, gets no suggestion. */
const suggestions: Partial<
  Record<LetteringCategory, { names: readonly string[]; italic?: boolean }>
> = {
  normal_dialogue: { names: ['CC Wild Words', 'New Wild Words', 'CC New Wild Words'] },
  thoughts: { names: ['Anime Ace 2.0 BB'], italic: true },
  floating: { names: ['Hogfish Demo'] },
  small_text: { names: ['Mangamaster Pro BB'] },
  narration_box: { names: ['CCTallTales'] },
  shout: { names: ['Palooka BB', 'Badaboom BB'] },
  sfx: { names: ['Carton Six', 'Darumadrop One'] },
  moan: { names: ['CDX Amraam'] },
  title: { names: ['BarberChop'] },
  phone_text: { names: ['CC Ultimatum'] },
}

/** A family's name as Suggest fonts compares names, as 0.0.4 did: in NFKC, without any space, in
 * lower case, so "CC Wild Words" finds the bundled "CCWildWords". */
function suggestionKey(name: string): string {
  return name.normalize('NFKC').replace(/\s+/g, '').toLowerCase()
}

/** `categories` with each category that has no entry, and a starting font among `families`, the
 * available ones, the bundled catalog's included, lettered in it under the family's own name, and
 * the categories so filled, in order; a category with an entry keeps it. */
export function suggestCategoryFonts(
  categories: CategoryFonts,
  families: readonly FontFamily[],
): { categories: CategoryFonts; filled: LetteringCategory[] } {
  // The first family by each name, as 0.0.4 took it.
  const available = new Map<string, string>()
  for (const { name } of families) {
    if (!available.has(suggestionKey(name))) available.set(suggestionKey(name), name)
  }
  const filled: LetteringCategory[] = []
  let next = categories
  for (const category of letteringCategories) {
    const suggestion = suggestions[category]
    if (!suggestion || next[category]) continue
    const family = suggestion.names
      .map((name) => available.get(suggestionKey(name)))
      .find((name) => name !== undefined)
    if (family === undefined) continue
    next = withEntry(next, category, { family, bold: false, italic: suggestion.italic ?? false })
    filled.push(category)
  }
  return { categories: next, filled }
}

/** Fills each category with no entry from the starting fonts among `families`. */
export function suggestFonts(families: readonly FontFamily[]): CategoryEdit {
  return (settings) => ({
    ...settings,
    categories: suggestCategoryFonts(settings.categories, families).categories,
  })
}

/** The card's part of `section`, as the desktop answers it with every key. */
function part(section: LetteringSettings | undefined): CategorySettings {
  return { auto: section?.auto ?? false, categories: section?.categories ?? {} }
}

/** The card's part as the last read or save left it in the shared settings. */
function read(): CategorySettings {
  const preferences = queryClient.getQueryData<BooharuPreferences>(booharuPreferencesKey)
  return part(preferences?.['booharu-lettering'])
}

/** Pick fonts automatically and the categories' own fonts as the settings file holds them, read
 * again each time a view showing them opens, with the changes being saved over them, and `change`,
 * which saves one at once, made to what the file holds just before, so the default text style,
 * colour detection, the small text and model confidence settings, and a category another writer
 * changed meanwhile, stay as the file has them, and answers whether it saved. Once the last save
 * ends, what the file holds shows again, so after a failed save, whose error shows as every failed
 * command's does, the file's values show. `settings` is undefined until the first read ends. */
export function useCategorySettings() {
  const preferences = useBooharuPreferences()
  const [unsaved, setUnsaved] = useState<CategorySettings | null>(null)
  /** Counts changes, so a save that ends knows whether a later change is still being saved. */
  const changes = useRef(0)

  const change = useCallback((edit: CategoryEdit): Promise<boolean> => {
    changes.current += 1
    const last = changes.current
    setUnsaved((shown) => edit(shown ?? read()))
    return saveBooharuSection('booharu-lettering', (section) => ({
      ...section,
      ...edit(part(section)),
    }))
      .then(
        () => true,
        () => false,
      )
      .finally(() => {
        if (changes.current === last) setUnsaved(null)
      })
  }, [])

  const section = preferences.data?.['booharu-lettering']
  return { settings: unsaved ?? (section ? part(section) : undefined), change }
}
