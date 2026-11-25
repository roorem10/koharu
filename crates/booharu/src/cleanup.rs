//! Cleaning exact shapes of a page at once (S14-p), as the agent's
//! `booharu_clean_areas` does: rectangles and polygons in page pixels,
//! rasterized onto one mask ([`area`]) and cleaned as one area by the
//! Inpainting stage alone ([`request`]), as Koharu's Remove brush cleans a
//! stroke and Generative Fill a marked area. The result goes where theirs
//! does, a new raster under all the page's lettering
//! (`booharu_core::cleanup::under_lettering`) limited to the area, so it
//! never covers a translation and never writes into a block's own clean-up,
//! which Redo boxes' Inpainting cleans again instead.
//!
//! A pixel is in a shape when its centre lies inside it: a rectangle holds
//! the centres from its left and top edges up to, but not on, its right and
//! bottom ones, and a polygon, its corners in order and closed back to the
//! first, the centres its outline winds around (nonzero winding, so a
//! polygon crossing itself fills every loop, one it winds around twice
//! too), a centre on its outline counting the same way, in on a left or top
//! side and out on a right or bottom one, on a slanted side exactly where
//! its corners lie on whole or half pixels within ten million pixels of the
//! page. So two shapes sharing an edge, or, between such corners, part of
//! one, share no pixel and leave none out between them. The shapes are
//! joined, each filled on its own, so one drawn the other way round adds to
//! the others as any does and never cuts a hole in them. The mask is 0 or
//! 255, with no growth, clipped to the page.

use std::collections::BTreeSet;

use anyhow::{Context as _, Result, bail, ensure};
use koharu_pipeline::{Area, PipelineConfig, Request, RunId, Stage, Target};
use koharu_scene::{PageId, Project, Rect};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A rectangle to clean, in page pixels: from its left edge `x` and its top
/// edge `y`, `width` pixels wide and `height` high. One with no width or no
/// height covers nothing.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rectangle {
    /// Its left edge, in pixels from the page's left.
    pub x: f64,
    /// Its top edge, in pixels from the page's top.
    pub y: f64,
    /// Its width, in pixels.
    #[schemars(range(min = 0.0))]
    pub width: f64,
    /// Its height, in pixels.
    #[schemars(range(min = 0.0))]
    pub height: f64,
}

/// A polygon's corner, in pixels from the page's top left.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// The shapes to clean on a page: rectangles, and polygons, each its
/// corners in order, at least 3, closed back to the first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shapes {
    pub rects: Vec<Rectangle>,
    pub polygons: Vec<Vec<Point>>,
}

/// What cleaning shapes answers at once: the run cleaning them, which goes
/// on in the background, the rectangle their area covers on the page, the
/// smallest holding every pixel cleaned, and how many pixels it cleans.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CleanAnswer {
    pub run: RunId,
    pub rect: Rect,
    pub pixels: u64,
}

/// `shapes` as the area of `page` in `project` they cover: its rectangle,
/// the smallest holding every pixel a shape holds, and a byte a pixel over
/// it, 255 in a shape and 0 elsewhere. Refused on a page the project doesn't
/// hold or with no scan, which the Inpainting stage would leave as it is,
/// with no shapes, a polygon of fewer than 3 corners, a value that isn't a
/// finite number, or shapes covering no pixel of the page.
pub fn area(project: &Project, page: PageId, shapes: &Shapes) -> Result<Area> {
    let at = project.page(page)?;
    ensure!(
        at.background.is_some(),
        "this page has no scan, so there is nothing to clean"
    );
    let outlines = outlines(shapes)?;
    let (width, height) = (at.size.width, at.size.height);
    outlines
        .iter()
        .filter_map(|outline| extent(outline))
        .reduce(|bounds, extent| bounds.union(extent))
        .and_then(|bounds| Rect::covering(bounds, width, height))
        .and_then(|candidate| filled(&outlines, candidate))
        .with_context(|| {
            format!(
                "the shapes cover no pixel of the page, which is {width} by {height} pixels: a \
                 pixel is cleaned when its centre lies inside a shape"
            )
        })
}

/// How many pixels `area` cleans.
#[must_use]
pub fn covered(area: &Area) -> u64 {
    area.pixels.iter().filter(|&&value| value != 0).count() as u64
}

/// The run cleaning `area` of `page` at once, as the Remove brush's does
/// with no layer selected: the Inpainting stage alone, on the area, with no
/// prompt, onto a new raster, and `config`, the pipeline's settings as
/// saved, so the inpainter set there cleans it. Its result's limit is the
/// area itself, 0 or 255, as no flag of Booharu's softens it.
#[must_use]
pub fn request(page: PageId, area: Area, config: PipelineConfig) -> Request {
    Request {
        target: Target::Area {
            page,
            area,
            prompt: None,
            layer: None,
        },
        stages: BTreeSet::from([Stage::Inpaint]),
        config: Some(config),
    }
}

/// A shape's outline: its corners in order, closed back to the first.
type Outline = Vec<(f64, f64)>;

/// Each shape of `shapes` as its outline, a rectangle's from its top left
/// clockwise, but none for a rectangle with no width or height. Refuses
/// what [`area`] refuses of the shapes.
fn outlines(shapes: &Shapes) -> Result<Vec<Outline>> {
    ensure!(
        !shapes.rects.is_empty() || !shapes.polygons.is_empty(),
        "there are no shapes to clean: give at least one rect or polygon"
    );
    let mut outlines = Vec::new();
    for (number, rect) in (1..).zip(&shapes.rects) {
        let Rectangle {
            x,
            y,
            width,
            height,
        } = *rect;
        let (right, bottom) = (x + width, y + height);
        ensure!(
            [x, y, width, height, right, bottom]
                .iter()
                .all(|value| value.is_finite()),
            "rect {number}'s x, y, width and height must be finite numbers, and so must its \
             right and bottom edges"
        );
        if width > 0.0 && height > 0.0 {
            outlines.push(vec![(x, y), (right, y), (right, bottom), (x, bottom)]);
        }
    }
    for (number, polygon) in (1..).zip(&shapes.polygons) {
        ensure!(
            polygon.len() >= 3,
            "polygon {number} has {} points; a polygon needs at least 3",
            polygon.len()
        );
        if let Some(corner) = polygon
            .iter()
            .position(|point| !(point.x.is_finite() && point.y.is_finite()))
        {
            bail!(
                "point {} of polygon {number} isn't at finite numbers",
                corner + 1
            );
        }
        outlines.push(polygon.iter().map(|point| (point.x, point.y)).collect());
    }
    Ok(outlines)
}

/// The smallest rectangle holding `outline`'s corners.
fn extent(outline: &[(f64, f64)]) -> Option<kurbo::Rect> {
    let (&first, rest) = outline.split_first()?;
    let start = kurbo::Rect::from_points(first, first);
    Some(rest.iter().fold(start, |bounds, &corner| {
        bounds.union_pt(kurbo::Point::from(corner))
    }))
}

/// The area `outlines` fill, each on its own by nonzero winding and joined,
/// within `candidate`, a rectangle of the page holding every pixel centre
/// they can hold: cut to the smallest rectangle holding what they fill, or
/// none when they fill nothing.
fn filled(outlines: &[Outline], candidate: Rect) -> Option<Area> {
    let (width, height) = (candidate.width as usize, candidate.height as usize);
    let (left, top) = (f64::from(candidate.x), f64::from(candidate.y));
    // The index among `count` pixels, from `start` on, of the first whose
    // centre lies at `at` or after it.
    let first = |at: f64, start: f64, count: usize| {
        (at - start - 0.5).ceil().clamp(0.0, count as f64) as usize
    };
    let mut mask = vec![0u8; width * height];
    let mut crossings = Vec::new();
    for outline in outlines {
        let Some(extent) = extent(outline) else {
            continue;
        };
        for row in first(extent.y0, top, height)..first(extent.y1, top, height) {
            crossed(outline, top + row as f64 + 0.5, &mut crossings);
            let line = &mut mask[row * width..(row + 1) * width];
            let mut winding = 0;
            for pair in crossings.windows(2) {
                let [(start, turn), (end, _)] = [pair[0], pair[1]];
                winding += turn;
                if winding != 0 {
                    // The pixels whose centres lie from `start` up to, but
                    // not on, `end`.
                    let (from, to) = (first(start, left, width), first(end, left, width));
                    line[from..to.max(from)].fill(255);
                }
            }
        }
    }
    tight(&mask, candidate)
}

/// Where `outline`'s edges cross the row of pixel centres at `centre`, left
/// to right, each with the way it winds, into `crossings`: an edge counts
/// from its upper end up to, but not at, its lower one, so a corner on the
/// row counts once where the outline goes on past the row, its two edges
/// cancel where the outline turns back there, and a level edge never
/// counts.
fn crossed(outline: &[(f64, f64)], centre: f64, crossings: &mut Vec<(f64, i32)>) {
    crossings.clear();
    let ends = outline.iter().zip(outline.iter().cycle().skip(1));
    for (&start, &end) in ends {
        let (turn, (upper_x, upper), (lower_x, lower)) = match start.1 < end.1 {
            true => (1, start, end),
            false => (-1, end, start),
        };
        if !(upper <= centre && centre < lower) {
            continue;
        }
        // An upright edge crosses every row at its own x, exactly, so a
        // centre on it is in or out by the rule on every row. Any other is
        // worked out from its upper end, so an edge two shapes share crosses
        // each row at the same place in both, whichever way each goes round
        // it, and product first, which is exact wherever an edge between
        // corners on whole or half pixels, within ten million pixels of the
        // page, runs through a centre, so a centre on a slanted edge is in
        // or out by the rule on every row too, and shapes sharing part of
        // such an edge leave none out between them. Corners so far apart
        // that the product overflows have the edge weighed between its ends
        // instead.
        let x = match upper_x == lower_x {
            true => upper_x,
            false => {
                let x = upper_x + (lower_x - upper_x) * (centre - upper) / (lower - upper);
                match x.is_finite() {
                    true => x,
                    false => {
                        let along = ((centre - upper) / (lower - upper)).clamp(0.0, 1.0);
                        upper_x * (1.0 - along) + lower_x * along
                    }
                }
            }
        };
        crossings.push((x, turn));
    }
    crossings.sort_by(|(a, _), (b, _)| a.total_cmp(b));
}

/// `mask`, a byte a pixel over `rect`, cut to the smallest rectangle
/// holding every pixel it fills, or none when it fills none.
fn tight(mask: &[u8], rect: Rect) -> Option<Area> {
    let width = rect.width as usize;
    let first = mask.iter().position(|&value| value != 0)?;
    let last = mask.iter().rposition(|&value| value != 0)?;
    let (top, bottom) = (first / width, last / width);
    let lines = || mask[top * width..(bottom + 1) * width].chunks_exact(width);
    let left = lines()
        .filter_map(|line| line.iter().position(|&value| value != 0))
        .min()?;
    let right = lines()
        .filter_map(|line| line.iter().rposition(|&value| value != 0))
        .max()?;
    let pixels = lines()
        .flat_map(|line| &line[left..=right])
        .copied()
        .collect();
    Some(Area {
        rect: Rect {
            x: rect.x + left as i32,
            y: rect.y + top as i32,
            width: (right - left + 1) as u32,
            height: (bottom - top + 1) as u32,
        },
        pixels,
    })
}

#[cfg(test)]
mod tests;
