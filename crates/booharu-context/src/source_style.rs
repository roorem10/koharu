use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SourceBold {
    Yes,
    No,
    Unsure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SourceDifference {
    Thin,
    Shaky,
    Heavy,
}

/// Measurements of the original detection mask, independent of translated typography.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize, Type)]
pub struct SourceMeasurements {
    pub stroke_ratio: f32,
    pub ink_density: f32,
    pub stroke_variation: f32,
    pub glyph_height: f32,
    pub glyph_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize, Type)]
pub struct SourceStyle {
    pub source_bold: SourceBold,
    pub source_different: bool,
    pub difference: Option<SourceDifference>,
    pub measurements: SourceMeasurements,
}

/// Measures an undilated, binary detection-mask crop; nonzero pixels are ink.
/// Filled text regions, tiny marks and screentone cannot establish a glyph size.
/// Returning `None` means unsure, rather than guessing a font from page artwork.
#[must_use]
pub fn measure_source_mask(width: u32, height: u32, pixels: &[u8]) -> Option<SourceMeasurements> {
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(height).ok()?;
    if width == 0 || height == 0 || width.checked_mul(height)? != pixels.len() {
        return None;
    }
    let mut visited = vec![false; pixels.len()];
    let mut pending = Vec::new();
    let mut glyphs = Vec::new();
    let mut total_ink = 0_usize;
    let mut accepted_ink = 0_usize;
    let mut accepted_bounds = 0_usize;
    for start in 0..pixels.len() {
        if pixels[start] == 0 || visited[start] {
            continue;
        }
        visited[start] = true;
        pending.push(start);
        let mut min_x = width;
        let mut min_y = height;
        let mut max_x = 0;
        let mut max_y = 0;
        let mut area = 0_usize;
        let mut perimeter = 0_usize;
        while let Some(index) = pending.pop() {
            let x = index % width;
            let y = index / width;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            area += 1;
            for (outside, adjacent) in [
                (x == 0, index.wrapping_sub(1)),
                (x + 1 == width, index + 1),
                (y == 0, index.wrapping_sub(width)),
                (y + 1 == height, index + width),
            ] {
                if outside || pixels[adjacent] == 0 {
                    perimeter += 1;
                }
            }
            for next_y in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for next_x in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                    let adjacent = next_y * width + next_x;
                    if pixels[adjacent] != 0 && !visited[adjacent] {
                        visited[adjacent] = true;
                        pending.push(adjacent);
                    }
                }
            }
        }
        total_ink += area;
        let glyph_height = max_y - min_y + 1;
        let glyph_width = max_x - min_x + 1;
        let bounds = glyph_height * glyph_width;
        let density = area as f32 / bounds as f32;
        if glyph_height < 6 || glyph_width < 2 || area < 12 || density > 0.88 {
            continue;
        }
        // Twice area/perimeter estimates a stroke's width without a skeleton or
        // dependence on crop padding. Divide per glyph before taking the median.
        let stroke_ratio = 2.0 * area as f32 / perimeter as f32 / glyph_height as f32;
        glyphs.push((stroke_ratio, glyph_height as f32));
        accepted_ink += area;
        accepted_bounds += bounds;
    }
    if glyphs.len() < 3 || accepted_ink as f32 / (total_ink.max(1) as f32) < 0.75 {
        return None;
    }
    let stroke_ratio = median(glyphs.iter().map(|glyph| glyph.0))?;
    let glyph_height = median(glyphs.iter().map(|glyph| glyph.1))?;
    let stroke_variation = median(glyphs.iter().map(|glyph| (glyph.0 - stroke_ratio).abs()))?
        / stroke_ratio;
    Some(SourceMeasurements {
        stroke_ratio,
        ink_density: accepted_ink as f32 / accepted_bounds as f32,
        stroke_variation,
        glyph_height,
        glyph_count: u32::try_from(glyphs.len()).ok()?,
    })
}

/// Compare with this page's dialogue, including the candidate when it is dialogue.
/// Three readable boxes establish a baseline; a uniformly heavy page stays normal.
/// A grey band between ordinary and bold deliberately produces no stored result.
#[must_use]
pub fn compare_source_style(
    measurements: SourceMeasurements,
    dialogue: &[SourceMeasurements],
) -> Option<SourceStyle> {
    if !valid_measurements(&measurements) {
        return None;
    }
    let dialogue = dialogue
        .iter()
        .filter(|value| valid_measurements(value))
        .collect::<Vec<_>>();
    if dialogue.len() < 3 {
        return None;
    }
    let stroke = measurements.stroke_ratio / median(dialogue.iter().map(|m| m.stroke_ratio))?;
    let density = measurements.ink_density / median(dialogue.iter().map(|m| m.ink_density))?;
    let variation = median(dialogue.iter().map(|m| m.stroke_variation))?;
    let source_bold = if stroke >= 1.3 && density >= 1.15 {
        SourceBold::Yes
    } else if stroke <= 1.1 {
        SourceBold::No
    } else {
        return None;
    };
    let difference = if source_bold == SourceBold::Yes || density >= 1.3 {
        Some(SourceDifference::Heavy)
    } else if measurements.stroke_variation >= 0.35
        && measurements.stroke_variation >= variation + 0.2
    {
        Some(SourceDifference::Shaky)
    } else if stroke <= 0.8 || density <= 0.75 {
        Some(SourceDifference::Thin)
    } else {
        None
    };
    Some(SourceStyle {
        source_bold,
        source_different: difference.is_some(),
        difference,
        measurements,
    })
}

fn valid_measurements(value: &SourceMeasurements) -> bool {
    value.glyph_count >= 3
        && value.glyph_height.is_finite()
        && value.glyph_height >= 6.0
        && value.stroke_ratio.is_finite()
        && value.stroke_ratio > 0.0
        && value.stroke_ratio < 0.5
        && value.ink_density.is_finite()
        && value.ink_density > 0.0
        && value.ink_density <= 1.0
        && value.stroke_variation.is_finite()
        && value.stroke_variation >= 0.0
}

fn median(values: impl Iterator<Item = f32>) -> Option<f32> {
    let mut values = values.collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    values.sort_unstable_by(f32::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) * 0.5
    } else {
        values[middle]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letters(strokes: &[u32], scale: u32) -> (u32, u32, Vec<u8>) {
        let width = (strokes.len() as u32 * 24 + 8) * scale;
        let height = 32 * scale;
        let mut mask = vec![0; (width * height) as usize];
        for (glyph, &stroke) in strokes.iter().enumerate() {
            let left = (glyph as u32 * 24 + 8) * scale;
            for y in 0..24 * scale {
                for x in 0..16 * scale {
                    if x < stroke * scale
                        || x >= (16 - stroke) * scale
                        || ((12 - stroke / 2) * scale..(12 + stroke - stroke / 2) * scale)
                            .contains(&y)
                    {
                        mask[((y + 4 * scale) * width + left + x) as usize] = 255;
                    }
                }
            }
        }
        (width, height, mask)
    }

    fn measured(stroke: u32) -> SourceMeasurements {
        let (width, height, mask) = letters(&[stroke; 5], 1);
        measure_source_mask(width, height, &mask).unwrap()
    }

    #[test]
    fn distinguishes_thin_and_thick_letters_relative_to_the_page() {
        let thin = measured(1);
        let normal = measured(2);
        let thick = measured(4);
        assert!(thick.stroke_ratio > normal.stroke_ratio);
        assert!(thick.ink_density > normal.ink_density);
        let page = [normal, normal, normal, thick];
        let bold = compare_source_style(thick, &page).unwrap();
        assert_eq!(bold.source_bold, SourceBold::Yes);
        assert_eq!(bold.difference, Some(SourceDifference::Heavy));
        let thin = compare_source_style(thin, &page).unwrap();
        assert_eq!(thin.source_bold, SourceBold::No);
        assert_eq!(thin.difference, Some(SourceDifference::Thin));
        let usual = compare_source_style(normal, &page).unwrap();
        assert_eq!(usual.source_bold, SourceBold::No);
        assert!(!usual.source_different);
    }

    #[test]
    fn an_all_bold_page_and_scaled_lettering_do_not_create_shouts() {
        let thick = measured(4);
        let same = compare_source_style(thick, &[thick; 5]).unwrap();
        assert_eq!(same.source_bold, SourceBold::No);
        assert!(!same.source_different);
        let normal = measured(2);
        let (width, height, mask) = letters(&[2; 5], 2);
        let scaled = measure_source_mask(width, height, &mask).unwrap();
        assert!((scaled.stroke_ratio - normal.stroke_ratio).abs() < 0.0001);
        assert!((scaled.ink_density - normal.ink_density).abs() < 0.0001);
        assert!(
            !compare_source_style(scaled, &[normal; 3])
                .unwrap()
                .source_different
        );
    }

    #[test]
    fn screentone_and_region_masks_are_unsure_instead_of_bold() {
        let mut noise = vec![0; 64 * 64];
        for y in (0..64).step_by(3) {
            for x in (0..64).step_by(3) {
                noise[y * 64 + x] = 255;
            }
        }
        assert_eq!(measure_source_mask(64, 64, &noise), None);
        assert_eq!(measure_source_mask(64, 64, &vec![255; 64 * 64]), None);
        assert_eq!(measure_source_mask(64, 64, &[255]), None);
        let (width, height, mut mask) = letters(&[2; 5], 1);
        for x in (0..width).step_by(3) {
            mask[x as usize] = 255;
        }
        let clean = measured(2);
        let with_noise = measure_source_mask(width, height, &mask).unwrap();
        assert_eq!(with_noise, clean);
    }

    #[test]
    fn ambiguous_or_insufficient_comparisons_record_nothing() {
        let normal = measured(2);
        assert_eq!(compare_source_style(normal, &[normal; 2]), None);
        let ambiguous = SourceMeasurements {
            stroke_ratio: normal.stroke_ratio * 1.2,
            ink_density: normal.ink_density * 1.1,
            ..normal
        };
        assert_eq!(compare_source_style(ambiguous, &[normal; 3]), None);
        let invalid = SourceMeasurements {
            stroke_ratio: f32::NAN,
            ..normal
        };
        assert_eq!(compare_source_style(invalid, &[normal; 3]), None);
    }

    #[test]
    fn pronounced_stroke_variation_is_a_conservative_shaky_clue() {
        let normal = measured(2);
        let (width, height, mask) = letters(&[1, 1, 2, 4, 4], 1);
        let shaky = measure_source_mask(width, height, &mask).unwrap();
        let result = compare_source_style(shaky, &[normal; 3]).unwrap();
        assert_eq!(result.source_bold, SourceBold::No);
        assert_eq!(result.difference, Some(SourceDifference::Shaky));
        let usual = compare_source_style(shaky, &[shaky; 3]).unwrap();
        assert!(!usual.source_different);
    }
}
