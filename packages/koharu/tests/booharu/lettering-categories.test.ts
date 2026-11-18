import { describe, expect, it } from 'vitest'

import {
  type CategoryFonts,
  type CategorySettings,
  categoryName,
  firstFont,
  letteredFont,
  letteringCategories,
  setFamily,
  setStyle,
  shapeNote,
  shownFamily,
  suggestCategoryFonts,
  suggestFonts,
} from '@/lib/booharu/categories'
import { resources, supportedLanguages } from '@/lib/i18n'

import { fontFamilies } from './fonts'

/** A family neither bold nor italic, as Suggest fonts writes one. */
const plain = (family: string) => ({ family, bold: false, italic: false })

/** 0.0.4's starting fonts, the first of each category's, all available. */
const startingFonts = fontFamilies(
  'CC Wild Words',
  'Anime Ace 2.0 BB',
  'Hogfish Demo',
  'Mangamaster Pro BB',
  'CCTallTales',
  'Palooka BB',
  'Carton Six',
  'CDX Amraam',
  'BarberChop',
  'CC Ultimatum',
)

describe('Booharu lettering categories', () => {
  it('lists the 18 categories in Settings order, the four a bubble gives with a note', () => {
    expect(letteringCategories).toEqual([
      'normal_dialogue',
      'thoughts',
      'floating',
      'small_text',
      'narration_box',
      'shout',
      'weak_shout',
      'sfx',
      'moan',
      'title',
      'phone_text',
      'whisper',
      'shy',
      'crying',
      'menacing',
      'handwritten',
      'sign',
      'electronic_voice',
    ])
    expect(Object.fromEntries(letteringCategories.map((c) => [c, shapeNote(c)]))).toMatchObject({
      normal_dialogue: 'booharu.lettering.shapeNotes.round',
      floating: 'booharu.lettering.shapeNotes.none',
      narration_box: 'booharu.lettering.shapeNotes.rectangle',
      shout: 'booharu.lettering.shapeNotes.spiky',
    })
    expect(letteringCategories.filter((category) => shapeNote(category) !== null)).toHaveLength(4)
  })

  it('takes the default font from the first Preferred font with a name', () => {
    expect(firstFont(['  ', ' CCWildWords ', 'Komika Text'])).toBe('CCWildWords')
    expect(firstFont([])).toBeUndefined()
    expect(firstFont(null)).toBeUndefined()
  })
})

describe('Booharu Suggest fonts', () => {
  it('finds a starting font under the name the font list gives, such as the catalog’s', () => {
    // 0.0.4's "CC Wild Words" is CCWildWords in the bundled catalog, which counts as available.
    const { categories, filled } = suggestCategoryFonts(
      {},
      fontFamilies('Komika Text', 'CCWildWords'),
    )
    expect(categories).toEqual({ normal_dialogue: plain('CCWildWords') })
    expect(filled).toEqual(['normal_dialogue'])
  })

  it('compares names in NFKC, without any space, ignoring case', () => {
    const { categories, filled } = suggestCategoryFonts(
      {},
      fontFamilies(
        'ＣＡＲＴＯＮ\u3000ＳＩＸ',
        'palooka\u00a0bb',
        'Mangamaster  Pro\tBB',
        'BARBER CHOP',
      ),
    )
    expect(categories).toEqual({
      small_text: plain('Mangamaster  Pro\tBB'),
      shout: plain('palooka\u00a0bb'),
      sfx: plain('ＣＡＲＴＯＮ\u3000ＳＩＸ'),
      title: plain('BARBER CHOP'),
    })
    expect(filled).toEqual(['small_text', 'shout', 'sfx', 'title'])
  })

  it('never changes a category that has a font, even one with only bold or italic', () => {
    const set: CategoryFonts = {
      normal_dialogue: plain('Komika Text'),
      thoughts: { family: null, bold: true, italic: false },
    }
    const { categories, filled } = suggestCategoryFonts(set, startingFonts)
    expect(categories.normal_dialogue).toEqual(plain('Komika Text'))
    expect(categories.thoughts).toEqual({ family: null, bold: true, italic: false })
    expect(filled).not.toContain('normal_dialogue')
    expect(filled).not.toContain('thoughts')
    expect(categories.floating).toEqual(plain('Hogfish Demo'))
  })

  it('gives Thoughts its starting font in italic', () => {
    expect(suggestCategoryFonts({}, fontFamilies('Anime Ace 2.0 BB')).categories).toEqual({
      thoughts: { family: 'Anime Ace 2.0 BB', bold: false, italic: true },
    })
  })

  it('suggests nothing for the categories without a starting font, the three new ones among them', () => {
    const { categories, filled } = suggestCategoryFonts({}, startingFonts)
    expect(filled).toEqual([
      'normal_dialogue',
      'thoughts',
      'floating',
      'small_text',
      'narration_box',
      'shout',
      'sfx',
      'moan',
      'title',
      'phone_text',
    ])
    for (const category of [
      'handwritten',
      'sign',
      'electronic_voice',
      'weak_shout',
      'whisper',
      'shy',
      'crying',
      'menacing',
    ] as const) {
      expect(categories).not.toHaveProperty(category)
    }
  })

  it('takes the first starting font available, and the first family by each name', () => {
    expect(suggestCategoryFonts({}, fontFamilies('Badaboom BB')).categories).toEqual({
      shout: plain('Badaboom BB'),
    })
    expect(
      suggestCategoryFonts({}, fontFamilies('Badaboom BB', 'Palooka BB')).categories.shout,
    ).toEqual(plain('Palooka BB'))
    expect(
      suggestCategoryFonts({}, fontFamilies('New Wild Words', 'CC New Wild Words')).categories,
    ).toEqual({ normal_dialogue: plain('New Wild Words') })
    expect(
      suggestCategoryFonts({}, fontFamilies('CC Wild Words', 'CCWildWords')).categories,
    ).toEqual({ normal_dialogue: plain('CC Wild Words') })
  })

  it('suggests nothing when no starting font is available', () => {
    expect(suggestCategoryFonts({}, fontFamilies('Arial', 'Komika Text'))).toEqual({
      categories: {},
      filled: [],
    })
    expect(suggestCategoryFonts({}, [])).toEqual({ categories: {}, filled: [] })
  })

  it('fills what the file holds just before the save, keeping the switch', () => {
    const settings: CategorySettings = {
      auto: true,
      categories: { shout: plain('Komika Text') },
    }
    expect(suggestFonts(fontFamilies('Palooka BB', 'Hogfish Demo'))(settings)).toEqual({
      auto: true,
      categories: { shout: plain('Komika Text'), floating: plain('Hogfish Demo') },
    })
  })
})

describe('Booharu category fonts', () => {
  const dialogue: CategoryFonts = { normal_dialogue: { family: 'Komika Text', bold: true } }

  it('letters a category in its own font, else Normal Dialogue’s, else the default font', () => {
    expect(letteredFont({}, 'whisper')).toEqual({ family: null, bold: false, italic: false })
    expect(letteredFont(dialogue, 'whisper')).toEqual({
      family: 'Komika Text',
      bold: true,
      italic: false,
    })
    expect(
      letteredFont({ ...dialogue, whisper: { family: 'Anime Ace', italic: true } }, 'whisper'),
    ).toEqual({ family: 'Anime Ace', bold: false, italic: true })
  })

  it('shows a category without its own font as the leading row, and one without a family as the default font', () => {
    expect(shownFamily({}, 'whisper', 'CCWildWords')).toBe('')
    expect(shownFamily(dialogue, 'whisper', 'CCWildWords')).toBe('')
    expect(shownFamily(dialogue, 'normal_dialogue', 'CCWildWords')).toBe('Komika Text')
    // Normal Dialogue without a family is the default font: its leading row says so.
    expect(
      shownFamily({ normal_dialogue: { family: null, italic: true } }, 'normal_dialogue', 'X'),
    ).toBe('')
    expect(shownFamily({ whisper: { family: null, italic: true } }, 'whisper', 'CCWildWords')).toBe(
      'CCWildWords',
    )
  })

  it('clears a category with the leading row, leaving the others', () => {
    const settings: CategorySettings = {
      auto: true,
      categories: { whisper: { family: 'Anime Ace', bold: true }, shout: plain('Komika Text') },
    }
    expect(setFamily('whisper', '')(settings)).toEqual({
      auto: true,
      categories: { shout: plain('Komika Text') },
    })
    // A category with nothing to clear stays so.
    expect(setFamily('moan', '')(settings)).toEqual(settings)
  })

  it('picks a family as bold and italic as the category is lettered', () => {
    expect(setFamily('whisper', 'Anime Ace')({ auto: false, categories: dialogue })).toEqual({
      auto: false,
      categories: { ...dialogue, whisper: { family: 'Anime Ace', bold: true, italic: false } },
    })
    expect(
      setFamily('normal_dialogue', 'Anime Ace')({ auto: false, categories: dialogue }).categories,
    ).toEqual({ normal_dialogue: { family: 'Anime Ace', bold: true, italic: false } })
  })

  it('writes the family a category is lettered in when its bold or italic changes', () => {
    const none: CategorySettings = { auto: false, categories: {} }
    // The default font by name, as the category's picker shows it.
    expect(setStyle('whisper', 'bold', true, 'CCWildWords')(none).categories).toEqual({
      whisper: { family: 'CCWildWords', bold: true, italic: false },
    })
    // Normal Dialogue's, which it was lettered in.
    expect(
      setStyle('whisper', 'italic', true, 'CCWildWords')({ auto: false, categories: dialogue })
        .categories.whisper,
    ).toEqual({ family: 'Komika Text', bold: true, italic: true })
    // Normal Dialogue keeps the default font as none, its copy.
    expect(setStyle('normal_dialogue', 'italic', true, 'CCWildWords')(none).categories).toEqual({
      normal_dialogue: { family: null, bold: false, italic: true },
    })
  })

  it('leaves out an entry with no family, bold or italic, which the section refuses', () => {
    const italic: CategorySettings = {
      auto: true,
      categories: { normal_dialogue: { family: null, bold: false, italic: true } },
    }
    expect(setStyle('normal_dialogue', 'italic', false, 'CCWildWords')(italic)).toEqual({
      auto: true,
      categories: {},
    })
  })
})

describe('Booharu lettering strings', () => {
  /** The value at the dotted `key` in `language`'s strings, Booharu's merged over Koharu's. */
  const lookup = (language: string, key: string): unknown =>
    key
      .split('.')
      .reduce<unknown>(
        (value, part) => (value as Record<string, unknown> | undefined)?.[part],
        resources[language as keyof typeof resources].translation,
      )

  /** Every string under `value`, by its dotted key. */
  const strings = (value: unknown, prefix = ''): Array<[string, string]> =>
    Object.entries(value as object).flatMap(([key, item]) => {
      const path = prefix ? `${prefix}.${key}` : key
      return typeof item === 'string' ? [[path, item]] : strings(item, path)
    })

  /** The `{{name}}` values a string interpolates, in order. */
  const values = (text: string) => Array.from(text.matchAll(/{{(\w+)}}/g), ([, name]) => name)

  it('names every category, each differently, and notes each shape, in all 12 languages', () => {
    expect(supportedLanguages).toHaveLength(12)
    for (const language of supportedLanguages) {
      const names = letteringCategories.map((category) => lookup(language, categoryName(category)))
      for (const name of names) expect(name, language).toEqual(expect.stringMatching(/\S/))
      // The picker finds a category by its name, so no two share one.
      expect(new Set(names).size, language).toBe(letteringCategories.length)
      for (const note of letteringCategories.map(shapeNote)) {
        if (note) expect(lookup(language, note), `${language}: ${note}`).toEqual(expect.any(String))
      }
    }
  })

  it('keeps the values each card string interpolates in every language', () => {
    const english = new Map(
      [
        ...strings(lookup('en-US', 'booharu.lettering'), 'booharu.lettering'),
        ...strings(lookup('en-US', 'booharu.typesetting.apply'), 'booharu.typesetting.apply'),
      ].map(([key, text]) => [key, values(text)]),
    )
    const plural = /_(zero|one|two|few|many|other)$/
    for (const language of supportedLanguages) {
      for (const [key, text] of [
        ...strings(lookup(language, 'booharu.lettering'), 'booharu.lettering'),
        ...strings(lookup(language, 'booharu.typesetting.apply'), 'booharu.typesetting.apply'),
      ]) {
        const source = english.get(key) ?? english.get(key.replace(plural, '_other'))
        expect(values(text).sort(), `${language}: ${key}`).toEqual([...(source ?? [])].sort())
      }
    }
  })
})
