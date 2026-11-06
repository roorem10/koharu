import type { FontFamily, LetteringCategory } from '@koharu/bridge/protocol'

export type FontMatch =
  | 'comic'
  | 'handwriting'
  | 'bold'
  | 'display'
  | 'plain'
  | 'italic'
  | 'category'

export type FontSuggestion = { family: FontFamily; score: number; match: FontMatch }

const categoryNames: Partial<Record<LetteringCategory, readonly string[]>> = {
  normal_dialogue: ['CC Wild Words', 'New Wild Words', 'CC New Wild Words'],
  thoughts: ['Anime Ace 2.0 BB'],
  floating: ['Hogfish Demo'],
  small_text: ['Mangamaster Pro BB'],
  narration_box: ['CCTallTales'],
  shout: ['Palooka BB', 'Badaboom BB'],
  sfx: ['Carton Six', 'Darumadrop One'],
  moan: ['CDX Amraam'],
  title: ['BarberChop'],
  phone_text: ['CC Ultimatum'],
}

const categoryMatches: Record<LetteringCategory, readonly FontMatch[]> = {
  normal_dialogue: ['comic', 'handwriting'],
  thoughts: ['italic', 'handwriting', 'comic'],
  floating: ['handwriting', 'comic'],
  small_text: ['handwriting', 'comic'],
  narration_box: ['comic', 'plain'],
  shout: ['bold', 'display', 'comic'],
  weak_shout: ['bold', 'comic'],
  sfx: ['display', 'bold', 'handwriting'],
  moan: ['handwriting', 'italic'],
  title: ['display', 'bold'],
  phone_text: ['plain'],
  whisper: ['handwriting', 'italic', 'comic'],
  shy: ['handwriting', 'italic'],
  crying: ['handwriting', 'italic'],
  menacing: ['display', 'bold'],
}

const patterns: Record<Exclude<FontMatch, 'category'>, RegExp> = {
  comic: /comic|manga|anime|wildwords|コミック|漫画|マンガ/,
  handwriting: /handwrit|handwritten|handletter|script|brush|casual|手書|手描/,
  bold: /bold|black|heavy|impact|thick|太字|極太/,
  display: /display|poster|headline|horror|gothic|タイトル/,
  plain: /sans|mono|gothic|ゴシック/,
  italic: /italic|oblique|cursive/,
}

function normalizedName(name: string): string {
  return name
    .normalize('NFKC')
    .toLowerCase()
    .replace(/[\s_-]+/g, '')
}

/** Name hints are suggestions, never an inference about a font's glyph coverage. */
export function rankCategoryFonts(
  category: LetteringCategory,
  families: readonly FontFamily[],
): FontSuggestion[] {
  const knownNames = (categoryNames[category] ?? []).map(normalizedName)
  return families
    .filter((family) => family.sources.includes('system'))
    .flatMap((family): FontSuggestion[] => {
      const names = [family.name, ...family.local_names].map(normalizedName)
      const knownIndex = knownNames.findIndex((known) => names.includes(known))
      if (knownIndex >= 0) return [{ family, score: 100 - knownIndex, match: 'category' }]

      const matches = categoryMatches[category]
      let score = 0
      let match: FontMatch | undefined
      for (const [index, hint] of matches.entries()) {
        if (hint !== 'category' && names.some((name) => patterns[hint].test(name))) {
          score += 10 - index
          match ??= hint
        }
      }
      return match ? [{ family, score, match }] : []
    })
    .sort(
      (left, right) =>
        right.score - left.score ||
        normalizedName(left.family.name).localeCompare(normalizedName(right.family.name), 'en') ||
        left.family.name.localeCompare(right.family.name, 'en'),
    )
}
