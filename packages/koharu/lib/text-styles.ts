import { rankCategoryFonts } from '@/components/ready-check/font-ranking'
import { LETTERING_CATEGORIES } from '@/lib/line-labels'
import type {
  CategoryStyles,
  FontFamily,
  LetteringCategory,
  ShapeStyles,
  TextStyle,
  TextStyles,
} from '@koharu/bridge/protocol'

export const emptyTextStyle: TextStyle = {
  font: null,
  size: null,
  fill: null,
  outline: null,
  outer_outline: null,
  alignment: null,
  writing_mode: null,
}
export const emptyShapeStyles = {
  enabled: false,
  rectangle: { ...emptyTextStyle },
  round: { ...emptyTextStyle },
  spiky: { ...emptyTextStyle },
  free: { ...emptyTextStyle },
} satisfies ShapeStyles

export const emptyCategoryStyles = {
  thoughts: { ...emptyTextStyle },
  small_text: { ...emptyTextStyle },
  weak_shout: { ...emptyTextStyle },
  sfx: { ...emptyTextStyle },
  moan: { ...emptyTextStyle },
  title: { ...emptyTextStyle },
  phone_text: { ...emptyTextStyle },
  whisper: { ...emptyTextStyle },
  shy: { ...emptyTextStyle },
  crying: { ...emptyTextStyle },
  menacing: { ...emptyTextStyle },
} satisfies CategoryStyles

export const categoryShapes: Partial<
  Record<LetteringCategory, keyof Omit<ShapeStyles, 'enabled'>>
> = {
  normal_dialogue: 'round',
  floating: 'free',
  narration_box: 'rectangle',
  shout: 'spiky',
}

export function categoryStyle(styles: TextStyles, category: LetteringCategory): TextStyle {
  const shape = categoryShapes[category]
  return shape
    ? ((styles.shape_styles ?? emptyShapeStyles)[shape] ?? emptyTextStyle)
    : ((styles.category_styles ?? emptyCategoryStyles)[category as keyof CategoryStyles] ??
        emptyTextStyle)
}

export function withCategoryStyle(
  styles: TextStyles,
  category: LetteringCategory,
  style: TextStyle,
): TextStyles {
  const shape = categoryShapes[category]
  return shape
    ? { ...styles, shape_styles: { ...emptyShapeStyles, ...styles.shape_styles, [shape]: style } }
    : {
        ...styles,
        category_styles: { ...emptyCategoryStyles, ...styles.category_styles, [category]: style },
      }
}

export function isEmptyStyle(style: TextStyle): boolean {
  return Object.values(style).every((group) => group === null || group === undefined)
}

export function suggestCategoryFonts(styles: TextStyles, families: FontFamily[]): TextStyles {
  let next = styles
  for (const category of LETTERING_CATEGORIES) {
    if (!isEmptyStyle(categoryStyle(next, category))) continue
    const family = rankCategoryFonts(category, families)[0]?.family
    if (!family) continue
    next = withCategoryStyle(next, category, {
      ...categoryStyle(next, category),
      font: {
        preferred_font: family.name,
        font_weight: null,
        font_style: category === 'thoughts' ? 'italic' : null,
      },
    })
  }
  return next
}
