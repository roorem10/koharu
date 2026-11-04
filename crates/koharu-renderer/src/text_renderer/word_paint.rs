use std::{collections::BTreeMap, ops::Range};

use koharu_scene::{RunStyle, char_range_to_byte_range};

use super::*;

pub(super) fn byte_runs(descriptor: &TextNodeDescriptor) -> Vec<Range<usize>> {
    descriptor
        .runs
        .iter()
        .map(|run| char_range_to_byte_range(&descriptor.text, run.start, run.end))
        .collect()
}

pub(super) fn run_at(ranges: &[Range<usize>], cluster: u32) -> Option<usize> {
    let index = ranges.partition_point(|range| range.end <= cluster as usize);
    ranges
        .get(index)
        .filter(|range| range.contains(&(cluster as usize)))
        .map(|_| index)
}

fn options_for(
    base: &TextRenderOptions,
    style: Option<&RunStyle>,
    scale: f32,
) -> TextRenderOptions {
    let mut options = base.clone();
    if let Some(style) = style {
        options.color = style.color.unwrap_or(base.color);
        options.fill_gradient = style
            .fill_gradient
            .clone()
            .or_else(|| base.fill_gradient.clone());
        let mut stroke = base.stroke.unwrap_or(StrokeOptions {
            color: [255; 4],
            width_px: 0.0,
            method: StrokeMethod::Contour,
            position: StrokePosition::Outside,
            join: StrokeJoin::Round,
            outer: None,
        });
        stroke.color = style.stroke_color.unwrap_or(stroke.color);
        stroke.width_px = style.stroke_width.unwrap_or(stroke.width_px);
        let outer = stroke.outer.unwrap_or(OuterStroke {
            color: [0, 0, 0, 255],
            width_px: 0.0,
        });
        stroke.outer = Some(OuterStroke {
            color: style.outer_stroke_color.unwrap_or(outer.color),
            width_px: style.outer_stroke_width.unwrap_or(outer.width_px),
        });
        options.stroke = Some(stroke);
    }
    if let Some(stroke) = &mut options.stroke {
        stroke.width_px *= scale;
        if let Some(outer) = &mut stroke.outer {
            outer.width_px *= scale;
        }
    }
    options
}

struct PaintGroup<'a> {
    run: Option<usize>,
    size: f32,
    layout: LayoutRun<'a>,
}

/// Preserve the original pens when extracting glyphs; removing a neighbour must not move a word.
fn groups<'a>(layout: &LayoutRun<'a>, ranges: &[Range<usize>]) -> Vec<PaintGroup<'a>> {
    let mut groups: Vec<PaintGroup<'a>> = Vec::new();
    for line in &layout.lines {
        let (mut pen_x, mut pen_y) = (0.0, 0.0);
        let mut start = 0;
        while start < line.glyphs.len() {
            let first = &line.glyphs[start];
            let run = run_at(ranges, first.cluster);
            let mut end = start + 1;
            while end < line.glyphs.len()
                && run_at(ranges, line.glyphs[end].cluster) == run
                && line.glyphs[end].font_size == first.font_size
            {
                end += 1;
            }
            let index = groups
                .iter()
                .position(|group| group.run == run && group.size == first.font_size)
                .unwrap_or_else(|| {
                    let mut empty = layout.clone();
                    empty.lines.clear();
                    groups.push(PaintGroup {
                        run,
                        size: first.font_size,
                        layout: empty,
                    });
                    groups.len() - 1
                });
            let mut fragment = line.clone();
            fragment.baseline = (line.baseline.0 + pen_x, line.baseline.1 + pen_y);
            fragment.glyphs = line.glyphs[start..end].to_vec();
            for glyph in &fragment.glyphs {
                pen_x += glyph.x_advance;
                pen_y -= glyph.y_advance;
            }
            groups[index].layout.lines.push(fragment);
            start = end;
        }
    }
    groups
}

/// One gradient area per complete run, across all its lines. Inherited gradients retain the
/// complete box's glyph bounds. Both outline passes finish before any fill is drawn.
pub(super) fn render(
    scene: &mut PreparedScene,
    layout: &LayoutRun<'_>,
    descriptor: &TextNodeDescriptor,
    base: &TextRenderOptions,
    transform: Affine,
    perspective: Option<&crate::warp::Perspective>,
) -> Result<Rect> {
    let ranges = byte_runs(descriptor);
    let groups = groups(layout, &ranges);
    let map = |mut path: BezPath| -> Result<BezPath> {
        if let Some(perspective) = perspective {
            path.apply_affine(transform);
            perspective.path(&path)
        } else {
            Ok(path)
        }
    };
    let draw_transform = if perspective.is_some() {
        Affine::IDENTITY
    } else {
        transform
    };
    let mut paths = Vec::new();
    let mut gradient_bounds = BTreeMap::new();
    let mut whole_bounds: Option<Rect> = None;
    for group in &groups {
        let path = text_path(&group.layout, base)?;
        let mapped = map(path.clone())?;
        if !mapped.is_empty() {
            let bounds = mapped.bounding_box();
            whole_bounds = Some(whole_bounds.map_or(bounds, |old| old.union(bounds)));
            gradient_bounds
                .entry(group.run)
                .and_modify(|old: &mut Rect| *old = old.union(bounds))
                .or_insert(bounds);
        }
        paths.push((path, mapped));
    }
    let mut outer_commands = Vec::new();
    let mut inner_commands = Vec::new();
    let mut fill_commands = Vec::new();
    let mut bounds = whole_bounds.unwrap_or(Rect::ZERO);
    for (group, (path, mut fill)) in groups.iter().zip(paths) {
        let style = group.run.map(|index| &descriptor.runs[index].style);
        let options = options_for(base, style, group.size / layout.font_size);
        if let Some(stroke) = options.stroke {
            let contours = union_contours(&path);
            let opaque = options
                .fill_gradient
                .as_ref()
                .map_or(options.color[3] == 255, |gradient| {
                    gradient.stops.iter().all(|stop| stop.color[3] == 255)
                });
            if let Some(outer) = stroke.visible_outer() {
                let region = map(outer_border(
                    &contours,
                    stroke,
                    outer,
                    opaque && (stroke.width_px == 0.0 || stroke.color[3] == 255),
                ))?;
                if !region.is_empty() {
                    bounds = bounds.union(region.bounding_box());
                    outer_commands.push(fill_command(&region, draw_transform, outer.color));
                }
            }
            if stroke.width_px > 0.0 && stroke.color[3] > 0 {
                // Keep inside/centred borders visible without painting them over neighbours.
                if stroke.method == StrokeMethod::Contour
                    && stroke.position != StrokePosition::Outside
                {
                    let inset = if stroke.position == StrokePosition::Inside {
                        stroke.width_px
                    } else {
                        stroke.width_px * 0.5
                    };
                    fill = map(contours_path(&offset_region(
                        &contours,
                        -f64::from(inset),
                        stroke.join,
                    )))?;
                }
                let border = if stroke.method == StrokeMethod::Expansion {
                    contours_path(&offset_region(
                        &contours,
                        f64::from(stroke.width_px),
                        stroke.join,
                    ))
                } else {
                    contour_border(
                        &contours,
                        stroke,
                        stroke.position == StrokePosition::Outside && opaque,
                    )
                };
                let border = map(border)?;
                if !border.is_empty() {
                    bounds = bounds.union(border.bounding_box());
                    inner_commands.push(fill_command(&border, draw_transform, stroke.color));
                }
            }
        }
        if !fill.is_empty() {
            fill_commands.push(if let Some(gradient) = &options.fill_gradient {
                let area = if style.is_some_and(|style| style.fill_gradient.is_some()) {
                    gradient_bounds[&group.run]
                } else {
                    whole_bounds.unwrap_or(Rect::ZERO)
                };
                gradient_command_in_bounds(&fill, draw_transform, options.color, gradient, area)
            } else {
                fill_command(&fill, draw_transform, options.color)
            });
        }
    }
    scene.commands.extend(outer_commands);
    scene.commands.extend(inner_commands);
    scene.commands.extend(fill_commands);
    Ok(draw_transform.transform_rect_bbox(bounds))
}
