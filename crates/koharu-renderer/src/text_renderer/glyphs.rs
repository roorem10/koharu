use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::{TextGlyphBox, TextStyleRun, fonts::Font};

fn char_offset(text: &str, byte: usize) -> u32 {
    text.char_indices()
        .take_while(|(offset, _)| *offset < byte)
        .count() as u32
}

pub(super) fn smallest_size(descriptor: &TextNodeDescriptor, layout: &LayoutRun<'_>) -> f32 {
    let ranges = word_paint::byte_runs(descriptor);
    layout
        .lines
        .iter()
        .flat_map(|line| &line.glyphs)
        .map(|glyph| {
            let scale = word_paint::run_at(&ranges, glyph.cluster)
                .and_then(|index| descriptor.runs[index].style.size_scale)
                .unwrap_or(1.0);
            glyph.font_size.min(layout.font_size * scale)
        })
        .reduce(f32::min)
        .unwrap_or(layout.font_size)
}

pub(super) fn style_runs(
    descriptor: &TextNodeDescriptor,
    layout: &LayoutRun<'_>,
    primary: &Font,
) -> Vec<TextStyleRun> {
    if descriptor.runs.is_empty() {
        return Vec::new();
    }
    let ranges = word_paint::byte_runs(descriptor);
    let glyphs: BTreeMap<_, _> = layout
        .lines
        .iter()
        .flat_map(|line| &line.glyphs)
        .map(|glyph| (glyph.cluster as usize, glyph))
        .collect();
    let mut boundaries = BTreeSet::from([0, descriptor.text.len()]);
    boundaries.extend(glyphs.keys().copied());
    for range in &ranges {
        boundaries.insert(range.start);
        boundaries.insert(range.end);
    }
    let boundaries: Vec<_> = boundaries
        .into_iter()
        .filter(|byte| descriptor.text.is_char_boundary(*byte))
        .collect();
    let mut runs: Vec<TextStyleRun> = Vec::new();
    for pair in boundaries.windows(2) {
        let byte = pair[0];
        let style =
            word_paint::run_at(&ranges, byte as u32).map(|index| &descriptor.runs[index].style);
        let glyph = glyphs.range(..=byte).next_back().map(|(_, glyph)| *glyph);
        let font = glyph.map_or(primary, |glyph| glyph.font);
        let gradient = style
            .and_then(|style| style.fill_gradient.as_ref())
            .or(descriptor.fill_gradient.as_ref());
        let color = gradient
            .and_then(|gradient| gradient.stops.first())
            .map(|stop| stop.color)
            .unwrap_or_else(|| {
                style
                    .and_then(|style| style.color)
                    .unwrap_or(descriptor.foreground_color)
            });
        let run = TextStyleRun {
            start: char_offset(&descriptor.text, byte),
            end: char_offset(&descriptor.text, pair[1]),
            post_script_font: font.post_script_name().to_owned(),
            font_size: glyph.map_or(layout.font_size, |glyph| glyph.font_size),
            color,
            faux_bold: font.synthetic_bold(),
            faux_italic: font.synthetic_skew().is_some(),
        };
        if let Some(previous) = runs.last_mut()
            && previous.end == run.start
            && previous.post_script_font == run.post_script_font
            && previous.font_size == run.font_size
            && previous.color == run.color
            && previous.faux_bold == run.faux_bold
            && previous.faux_italic == run.faux_italic
        {
            previous.end = run.end;
        } else {
            runs.push(run);
        }
    }
    runs
}

pub(super) fn boxes(
    descriptor: &TextNodeDescriptor,
    layout: &LayoutRun<'_>,
    primary: &Font,
    transform: Affine,
) -> Result<Vec<TextGlyphBox>> {
    let perspective = match &descriptor.warp {
        Some(TextWarp::Perspective { corners }) => Some(crate::warp::Perspective::new(
            *corners,
            descriptor.width,
            descriptor.height,
        )?),
        _ => None,
    };
    let map = |point: vello::kurbo::Point| -> Result<[f32; 2]> {
        let point = transform * point;
        let point = if let Some(perspective) = &perspective {
            perspective.point(point)?
        } else {
            point
        };
        Ok([point.x as f32, point.y as f32])
    };
    let mut boxes = Vec::new();
    for (index, line) in layout.lines.iter().enumerate() {
        let base_metrics = primary
            .skrifa_ref()?
            .metrics(Size::new(layout.font_size), primary.location());
        let (mut ascent, mut descent) = if line.glyphs.is_empty() {
            (base_metrics.ascent, -base_metrics.descent)
        } else {
            (0.0_f32, 0.0_f32)
        };
        for glyph in &line.glyphs {
            let metrics = glyph
                .font
                .skrifa_ref()?
                .metrics(Size::new(glyph.font_size), glyph.font.location());
            ascent = ascent.max(metrics.ascent + glyph.y_offset);
            descent = descent.max(-metrics.descent - glyph.y_offset);
        }
        let (mut pen_x, mut pen_y) = line.baseline;
        let mut clusters: BTreeMap<usize, Rect> = BTreeMap::new();
        for glyph in &line.glyphs {
            let next_x = pen_x + glyph.x_advance;
            let next_y = pen_y - glyph.y_advance;
            let rect = if descriptor.writing_mode == WritingMode::Horizontal {
                Rect::new(
                    f64::from(pen_x.min(next_x)),
                    f64::from(line.baseline.1 - ascent),
                    f64::from(pen_x.max(next_x)),
                    f64::from(line.baseline.1 + descent),
                )
            } else {
                Rect::new(
                    f64::from(line.baseline.0 - ascent),
                    f64::from(pen_y.min(next_y)),
                    f64::from(line.baseline.0 + descent),
                    f64::from(pen_y.max(next_y)),
                )
            };
            clusters
                .entry(glyph.cluster as usize)
                .and_modify(|old| *old = old.union(rect))
                .or_insert(rect);
            pen_x = next_x;
            pen_y = next_y;
        }
        let entries: Vec<_> = clusters.into_iter().collect();
        for (cluster, &(start, rect)) in entries.iter().enumerate() {
            let end = entries
                .get(cluster + 1)
                .map_or(line.range.end, |entry| entry.0)
                .max(start)
                .min(descriptor.text.len());
            if start >= descriptor.text.len() || start >= end {
                continue;
            }
            boxes.push(TextGlyphBox {
                start: char_offset(&descriptor.text, start),
                end: char_offset(&descriptor.text, end),
                line: index as u32,
                quad: [
                    map((rect.x0, rect.y0).into())?,
                    map((rect.x1, rect.y0).into())?,
                    map((rect.x1, rect.y1).into())?,
                    map((rect.x0, rect.y1).into())?,
                ],
            });
        }
        let caret = if descriptor.writing_mode == WritingMode::Horizontal {
            // The logical line end is on the left in an RTL line.
            let x = entries.last().map_or(pen_x, |(_, rect)| {
                let rtl = crate::script::shaping_direction_for_text(
                    descriptor.text.get(line.range.clone()).unwrap_or_default(),
                    descriptor.writing_mode,
                )
                .0 == harfrust::Direction::RightToLeft;
                if (entries.len() > 1 && entries[0].1.x0 > rect.x0) || (entries.len() == 1 && rtl) {
                    rect.x0 as f32
                } else {
                    rect.x1 as f32
                }
            });
            Rect::new(
                f64::from(x),
                f64::from(line.baseline.1 - ascent),
                f64::from(x),
                f64::from(line.baseline.1 + descent),
            )
        } else {
            Rect::new(
                f64::from(line.baseline.0 - ascent),
                f64::from(pen_y),
                f64::from(line.baseline.0 + descent),
                f64::from(pen_y),
            )
        };
        let end = char_offset(&descriptor.text, line.range.end.min(descriptor.text.len()));
        boxes.push(TextGlyphBox {
            start: end,
            end,
            line: index as u32,
            quad: [
                map((caret.x0, caret.y0).into())?,
                map((caret.x1, caret.y0).into())?,
                map((caret.x1, caret.y1).into())?,
                map((caret.x0, caret.y1).into())?,
            ],
        });
    }
    Ok(boxes)
}
