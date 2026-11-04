import type { FontFamily, FontStyle, Typography } from '@koharu/bridge/protocol'

export const defaultFont: FontFamily = {
  name: 'CCWildWords',
  metadata: {
    primary_script: 'latn',
    scripts: ['latn'],
    languages: ['en'],
    category: 'HANDWRITING',
    classifications: ['comic', 'dialogue'],
    use_cases: ['comic-dialogue', 'word-balloons'],
  },
  local_names: [],
  sources: ['bundled'],
  faces: [
    {
      postscript_name: 'CCWildWords-Regular',
      weight: 400,
      weight_range: null,
      style: 'normal',
    },
  ],
}

export const defaultTypography: Typography = {
  preferred_font: null,
  font_weight: 400,
  font_style: 'normal',
  size: null,
  auto_fit: true,
  color: [0, 0, 0, 255],
  stroke_color: [255, 255, 255, 255],
  stroke_width: 0,
  stroke_method: null,
  stroke_position: null,
  stroke_join: null,
  outer_stroke_color: [0, 0, 0, 255],
  outer_stroke_width: 0,
  line_spacing: null,
  letter_spacing: null,
  fill_gradient: null,
  alignment: null,
  writing_mode: null,
}

export function findFontFamily(families: FontFamily[], name: string): FontFamily | undefined {
  return families.find((family) => normalizeFontName(family.name) === normalizeFontName(name))
}

export function usableFontStyles(family: FontFamily | undefined): FontStyle[] {
  if (!family) return ['normal']
  const styles = new Set(family.faces.map((font) => font.style))
  const available = (['normal', 'italic', 'oblique'] satisfies FontStyle[]).filter((style) =>
    styles.has(style),
  )
  return available.length ? available : ['normal']
}

export function usableFontWeights(family: FontFamily | undefined, style: FontStyle): number[] {
  if (!family) return [400]
  const styled = family.faces.filter((font) => font.style === style)
  const faces = styled.length ? styled : family.faces
  const weights = new Set(faces.map((font) => font.weight))
  for (const face of faces) {
    if (!face.weight_range) continue
    weights.add(face.weight_range.minimum)
    weights.add(face.weight_range.maximum)
    for (let weight = 100; weight <= 900; weight += 100) {
      if (weight >= face.weight_range.minimum && weight <= face.weight_range.maximum) {
        weights.add(weight)
      }
    }
  }
  return weights.size ? [...weights].sort((left, right) => left - right) : [400]
}

export function nearestFontWeight(weights: number[], target: number): number {
  return (weights.length ? weights : [400]).reduce((nearest, weight) =>
    Math.abs(weight - target) < Math.abs(nearest - target) ? weight : nearest,
  )
}

/**
 * Snaps a weight to a real face on the same side of bold (600). When the family has no
 * face on that side, the weight is kept and the renderer draws synthetic bold.
 */
export function styledFontWeight(
  family: FontFamily | undefined,
  style: FontStyle,
  target: number,
): number {
  const heavy = target >= 600
  const side = usableFontWeights(family, style).filter((weight) => weight >= 600 === heavy)
  return side.length ? nearestFontWeight(side, target) : target
}

export function normalizeFontName(value: string): string {
  return value.normalize('NFKC').trim().replace(/\s+/g, ' ').toLowerCase()
}

// Without a visible border, use a contrasting fill instead of transparent text.
export function invertBorder(value: Typography, displayedWidth: number): Typography {
  const color = value.fill_gradient?.stops[0]?.color ?? value.color ?? defaultTypography.color!
  const stroke = value.stroke_color
  if ((value.stroke_width ?? 0) > 0 && stroke && stroke[3] > 0) {
    return { ...value, color: stroke, stroke_color: color, fill_gradient: null }
  }
  const [red, green, blue] = color
  const light = 0.2126 * red + 0.7152 * green + 0.0722 * blue > 127.5
  return {
    ...value,
    color: light ? [0, 0, 0, 255] : [255, 255, 255, 255],
    fill_gradient: null,
    stroke_color: [red, green, blue, 255],
    stroke_width: displayedWidth,
  }
}
