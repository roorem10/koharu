//! The brush tools' feel on the canvas (`BrushFeel`): the brush a stroke
//! takes, Draw's at its opacity ([`felt`]), and a soft stroke ([`Soft`]).
//! Below a hardness of 1, a stroke's coverage falls off from its centre line
//! as Booharu 0.0.4's did, in place of Koharu's antialiased outline, and its
//! preview shows those bytes, tinted, as its result will have them; at 1 a
//! stroke is Koharu's own, byte for byte.

use std::{collections::BTreeMap, sync::Arc};

use koharu_scene::{LayerId, Op, Page, PixelFormat, Rgba};
use kurbo::{BezPath, Point, Shape as _, Size, Vec2};
use vello_gpu::{PixelMetadata, Pixmap, peniko::ImageAlphaType};

use crate::{Brush, Tool, booharu::BooharuSettings, draw::overlay::Overlay, gesture::Context};

/// The side of a soft stroke's tiles, in page pixels.
const TILE: u32 = 256;

/// `brush` as `tool`'s strokes take it under `settings`: for Draw, its
/// colour's alpha times Draw's opacity, which the preview's tint, the paint
/// on a raster and the uncovering of a mask all read, so a stroke paints as
/// one pass at that opacity, never building up within itself, as
/// Photoshop's Opacity does; for every other tool, as it is, as the Eraser
/// and Remove take no opacity.
pub(crate) fn felt(brush: Brush, tool: Tool, settings: &BooharuSettings) -> Brush {
    match tool {
        Tool::Draw => {
            let Rgba([r, g, b, a]) = brush.color;
            let opacity = settings.brush.draw_opacity as f32;
            Brush {
                color: Rgba([r, g, b, a * opacity]),
                ..brush
            }
        }
        _ => brush,
    }
}

/// The coverage of a stroke whose tool's hardness is below 1, a byte a page
/// pixel, as Booharu 0.0.4's clean-up strokes had it: full along the
/// stroke's centre line, the polyline through its points or its one point,
/// and falling off to nothing half a pixel past its radius, over a falloff
/// that grows as the hardness drops. It's kept in tiles `TILE` pixels
/// square, cut at the page's right and bottom edges, only where it reaches,
/// each with its preview: its coverage in the stroke's tint, premultiplied,
/// as the renderer holds the pixels the stroke paints.
pub(crate) struct Soft {
    /// The stroke's tool and what it strokes onto, which pick its tint.
    tool: Tool,
    target: Option<LayerId>,
    /// The brush's diameter its coverage was made at, in page pixels.
    diameter: f64,
    /// From 0 to below 1.
    hardness: f64,
    /// The page's width and height in pixels, which no coverage passes.
    page: (u32, u32),
    /// By their column and row.
    tiles: BTreeMap<(u32, u32), Tile>,
    /// What the previews are tinted in.
    tint: Rgba,
}

/// A tile of a soft stroke's coverage, with its preview.
struct Tile {
    width: u32,
    height: u32,
    /// A byte a pixel, row by row.
    coverage: Vec<u8>,
    /// The coverage tinted; none since it or the tint changed, until it's
    /// made again.
    preview: Option<Arc<Pixmap>>,
}

impl Soft {
    /// The coverage of a stroke of `cx`'s tool onto `target`, starting at
    /// `point` with its first dot; none at a hardness of 1, where the stroke
    /// is Koharu's own, or for a brush with no size.
    pub(crate) fn start(cx: &Context, target: Option<LayerId>, point: Point) -> Option<Self> {
        let hardness = cx.booharu.brush.hardness(cx.tool);
        let diameter = cx.brush.diameter;
        (hardness < 1.0 && diameter > 0.0 && diameter.is_finite()).then(|| {
            let tint = tint(cx, cx.tool, target);
            let page = cx.snapshot.size();
            let mut soft = Self::new(cx.tool, target, diameter, hardness, page, tint);
            soft.stroke(&[point]);
            soft.refresh();
            soft
        })
    }

    /// No coverage yet of a stroke `diameter` pixels wide at `hardness` on a
    /// page of `page`, to preview in `tint`.
    fn new(
        tool: Tool,
        target: Option<LayerId>,
        diameter: f64,
        hardness: f64,
        page: Size,
        tint: Rgba,
    ) -> Self {
        Self {
            tool,
            target,
            diameter,
            hardness: hardness.clamp(0.0, 1.0),
            page: (page.width as u32, page.height as u32),
            tiles: BTreeMap::new(),
            tint,
        }
    }

    /// Follows the stroke as its `points`, of which the first `before` were
    /// there already, grow, as `cx` shows it now: its new segments added,
    /// and its previews tinted as the stroke shows.
    pub(crate) fn moved(&mut self, cx: &Context, points: &[Point], before: usize) {
        self.grow(cx.brush.diameter, points, before);
        self.tinted(tint(cx, self.tool, self.target));
        self.refresh();
    }

    /// Takes in the stroke's `points` after the first `before`: each new
    /// segment, none made again, or at a brush `diameter` other than its own
    /// every segment again, as Koharu expands a stroke's whole outline again
    /// at the brush's size whenever a point arrives.
    fn grow(&mut self, diameter: f64, points: &[Point], before: usize) {
        if points.len() <= before {
            return;
        }
        if diameter == self.diameter {
            self.stroke(&points[before.saturating_sub(1)..]);
        } else {
            self.diameter = diameter;
            self.tiles.clear();
            self.stroke(points);
        }
    }

    /// Adds the stroke through `points`: each segment between two of them,
    /// or a dot for one.
    fn stroke(&mut self, points: &[Point]) {
        match points {
            [] => {}
            [point] => self.segment(*point, *point),
            _ => {
                for pair in points.windows(2) {
                    self.segment(pair[0], pair[1]);
                }
            }
        }
    }

    /// Adds the segment from `start` to `end`, a dot where they meet: each
    /// page pixel it reaches keeps the greater of its coverage and the
    /// segment's, so the order segments come in doesn't matter. A tile whose
    /// coverage rose shows it once it's refreshed.
    fn segment(&mut self, start: Point, end: Point) {
        let radius = self.diameter * 0.5;
        let finite = [start.x, start.y, end.x, end.y, radius]
            .iter()
            .all(|value| value.is_finite());
        if !finite || radius <= 0.0 {
            return;
        }
        let falloff = falloff(radius, self.hardness);
        let reach = radius + 0.5;
        let (width, height) = self.page;
        let left = (start.x.min(end.x) - reach).floor().max(0.0) as u32;
        let top = (start.y.min(end.y) - reach).floor().max(0.0) as u32;
        let right = ((start.x.max(end.x) + reach).ceil().max(0.0) as u32).min(width);
        let bottom = ((start.y.max(end.y) + reach).ceil().max(0.0) as u32).min(height);
        if left >= right || top >= bottom {
            return;
        }
        for row in top / TILE..=(bottom - 1) / TILE {
            for column in left / TILE..=(right - 1) / TILE {
                let (x0, y0) = (column * TILE, row * TILE);
                let key = (column, row);
                let new = !self.tiles.contains_key(&key);
                let tile = self
                    .tiles
                    .entry(key)
                    .or_insert_with(|| Tile::new(x0, y0, (width, height)));
                let mut rose = false;
                for y in top.max(y0)..bottom.min(y0 + tile.height) {
                    let line = ((y - y0) * tile.width) as usize;
                    let centre_y = f64::from(y) + 0.5;
                    for x in left.max(x0)..right.min(x0 + tile.width) {
                        let pixel = &mut tile.coverage[line + (x - x0) as usize];
                        // Nothing comes over full coverage.
                        if *pixel == u8::MAX {
                            continue;
                        }
                        let centre = Point::new(f64::from(x) + 0.5, centre_y);
                        let value = covered(distance(centre, start, end), radius, falloff);
                        if value > *pixel {
                            *pixel = value;
                            rose = true;
                        }
                    }
                }
                if rose {
                    tile.preview = None;
                } else if new {
                    self.tiles.remove(&key);
                }
            }
        }
    }

    /// Previews in `tint` from now on, every tile made again when it's
    /// another.
    fn tinted(&mut self, tint: Rgba) {
        if tint != self.tint {
            self.tint = tint;
            for tile in self.tiles.values_mut() {
                tile.preview = None;
            }
        }
    }

    /// Makes the previews of the tiles that changed, each a new pixmap, so
    /// the renderer uploads only those again: it keeps a texture by its
    /// pixmap's address, and lets go of it once nothing holds the pixmap.
    fn refresh(&mut self) {
        let tint = self.tint;
        for tile in self.tiles.values_mut() {
            if tile.preview.is_none() {
                tile.preview = Some(Arc::new(tile.tinted(tint)));
            }
        }
    }

    /// Its coverage over the page pixels Koharu's `outline` of the stroke
    /// touches, the rect `coverage` gives Koharu's stroke, so a soft stroke
    /// writes, or asks to clean, the same pixels. Every pixel it covers lies
    /// in that rect: a pixel it covers has its centre less than half a pixel
    /// past its radius from the centre line, so inside the outline's bounds
    /// or less than half a pixel past them.
    fn coverage(&self, outline: &BezPath, page: Size) -> Option<(koharu_scene::Rect, Vec<u8>)> {
        let rect = koharu_scene::Rect::covering(
            outline.bounding_box(),
            page.width as u32,
            page.height as u32,
        )?;
        // As `coverage` takes none too large for its renderer.
        u16::try_from(rect.width).ok()?;
        u16::try_from(rect.height).ok()?;
        let (left, top) = (rect.x as u32, rect.y as u32);
        let (right, bottom) = (left + rect.width, top + rect.height);
        let mut bytes = vec![0; rect.width as usize * rect.height as usize];
        for (&(column, row), tile) in &self.tiles {
            let (x0, y0) = (column * TILE, row * TILE);
            let (from, to) = (left.max(x0), right.min(x0 + tile.width));
            if from >= to {
                continue;
            }
            let count = (to - from) as usize;
            for y in top.max(y0)..bottom.min(y0 + tile.height) {
                let source = ((y - y0) * tile.width + (from - x0)) as usize;
                let target = ((y - top) * rect.width + (from - left)) as usize;
                bytes[target..target + count]
                    .copy_from_slice(&tile.coverage[source..source + count]);
            }
        }
        Some((rect, bytes))
    }

    /// Draws its tiles' previews over the page, a page pixel to each of
    /// their pixels; it fails when the GPU has no memory for one.
    pub(crate) fn preview(&self, overlay: &mut Overlay) -> anyhow::Result<()> {
        for (&(column, row), tile) in &self.tiles {
            if let Some(preview) = &tile.preview {
                let at = Vec2::new(f64::from(column * TILE), f64::from(row * TILE));
                overlay.booharu_pixmap(preview, at)?;
            }
        }
        Ok(())
    }
}

impl Tile {
    /// An empty tile with its corner at page pixel (`x`, `y`), cut at the
    /// edges of a page `width` by `height` pixels.
    fn new(x: u32, y: u32, (width, height): (u32, u32)) -> Self {
        let (width, height) = (TILE.min(width - x), TILE.min(height - y));
        Self {
            width,
            height,
            coverage: vec![0; (width * height) as usize],
            preview: None,
        }
    }

    /// Its coverage in `tint`, as a stroke paints it: the tint's colour at
    /// its alpha times the coverage, premultiplied as the renderer
    /// premultiplies a raster's pixels.
    fn tinted(&self, Rgba([r, g, b, a]): Rgba) -> Pixmap {
        let [r, g, b] = [r, g, b].map(|channel| (channel * 255.0).round() as u8);
        let mut pixels = vec![0; self.coverage.len() * 4];
        let (each, _) = pixels.as_chunks_mut::<4>();
        for (pixel, &covered) in each.iter_mut().zip(&self.coverage) {
            *pixel = [r, g, b, (f32::from(covered) * a).round() as u8];
        }
        Pixmap::from_parts(
            pixels,
            self.width as u16,
            self.height as u16,
            PixelMetadata {
                may_have_transparency: true,
                alpha_type: ImageAlphaType::Alpha,
            },
        )
    }
}

/// A stroke's coverage over the page pixels its `outline` touches, as its
/// edit or its clean-up takes it: `soft`'s when it's soft, else Koharu's,
/// the outline antialiased.
pub(crate) fn coverage(
    soft: Option<&Soft>,
    outline: &BezPath,
    cx: &Context,
) -> Option<(koharu_scene::Rect, Vec<u8>)> {
    let page = cx.snapshot.size();
    match soft {
        Some(soft) => soft.coverage(outline, page),
        None => crate::tools::brush::coverage(outline, page),
    }
}

/// `add`, the op adding the raster a Remove stroke with no raster or
/// clean-up selected goes onto (Koharu 0.95), at the top of `page`, put
/// under all its lettering instead, where an area's result goes (D17,
/// `booharu_core::cleanup::under_lettering`): above the scan, the Original
/// scan and earlier results, so the next stroke, which goes onto it while
/// it stays selected, builds on them, and under every layer that letters
/// the page. Any other op, as it is.
pub(crate) fn under_lettering(page: &Page, add: Op) -> Op {
    match add {
        Op::AddLayer {
            id,
            page: on,
            parent: None,
            layer,
            ..
        } => Op::AddLayer {
            id,
            page: on,
            parent: None,
            index: booharu_core::cleanup::under_lettering(page),
            layer,
        },
        other => other,
    }
}

/// What a stroke of `tool` onto `target` previews in, as Koharu's
/// `Stroke::preview` tints its hard stroke, which this follows: the brush's
/// colour, at Draw's opacity, for Draw onto a raster or onto nothing, and
/// otherwise, as a mask takes coverage, not colour, and the Eraser and
/// Remove take no colour, the selection's colour at 0.4.
fn tint(cx: &Context, tool: Tool, target: Option<LayerId>) -> Rgba {
    let colored = target
        .and_then(|layer| cx.snapshot.writable(layer))
        .is_none_or(|(_, format)| format == PixelFormat::Rgba8);
    match tool {
        Tool::Draw if colored => cx.brush.color,
        _ => {
            let Rgba([r, g, b, _]) = cx.colors.selection;
            Rgba([r, g, b, 0.4])
        }
    }
}

/// How far a soft stroke's coverage takes to fall from full to nothing, in
/// page pixels, at `radius` and `hardness`: Booharu 0.0.4's, the radius's
/// soft part, a pixel at least.
fn falloff(radius: f64, hardness: f64) -> f64 {
    (radius * (1.0 - hardness) - 0.5).max(1.0)
}

/// The coverage of a pixel whose centre lies `distance` from a stroke's
/// centre line, `radius` its radius and `falloff` its falloff, as Booharu
/// 0.0.4 gave it: full until `falloff` short of half a pixel past the
/// radius, then falling evenly to nothing there.
fn covered(distance: f64, radius: f64, falloff: f64) -> u8 {
    (((radius + 0.5 - distance) / falloff).clamp(0.0, 1.0) * 255.0).round() as u8
}

/// How far `point` lies from the segment from `start` to `end`, as Booharu
/// 0.0.4 measured it.
fn distance(point: Point, start: Point, end: Point) -> f64 {
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let length = dx.mul_add(dx, dy * dy);
    let along = if length <= f64::EPSILON {
        0.0
    } else {
        (((point.x - start.x) * dx + (point.y - start.y) * dy) / length).clamp(0.0, 1.0)
    };
    (point.x - (start.x + along * dx)).hypot(point.y - (start.y + along * dy))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::time::Instant;

    use super::*;
    use crate::booharu::BrushFeel;

    /// A brush 12 pixels wide in red at three quarters.
    const RED: Brush = Brush {
        diameter: 12.0,
        color: Rgba([1.0, 0.0, 0.0, 0.75]),
    };

    /// Settings whose brush feel is `feel`.
    fn feeling(feel: BrushFeel) -> BooharuSettings {
        BooharuSettings {
            brush: feel,
            ..BooharuSettings::default()
        }
    }

    #[test]
    fn draw_paints_at_its_opacity_and_no_other_tool_takes_it() {
        let half = feeling(BrushFeel {
            draw_opacity: 0.5,
            ..BrushFeel::default()
        });
        assert_eq!(
            felt(RED, Tool::Draw, &half),
            Brush {
                color: Rgba([1.0, 0.0, 0.0, 0.375]),
                ..RED
            }
        );
        for tool in [Tool::Eraser, Tool::Remove, Tool::Select, Tool::Pan] {
            assert_eq!(felt(RED, tool, &half), RED, "{tool:?}");
        }
        // At 1, as Koharu's brush is until the host pushes a feel, the
        // brush is as it was for every tool.
        let opaque = BooharuSettings::default();
        for tool in [Tool::Draw, Tool::Eraser, Tool::Remove] {
            assert_eq!(felt(RED, tool, &opaque), RED, "{tool:?}");
        }
    }

    #[test]
    fn a_stroke_takes_its_own_tools_hardness() {
        let feel = BrushFeel {
            draw_hardness: 0.25,
            eraser_hardness: 0.5,
            remove_hardness: 0.75,
            draw_opacity: 0.1,
        };
        assert_eq!(feel.hardness(Tool::Draw), 0.25);
        assert_eq!(feel.hardness(Tool::Eraser), 0.5);
        assert_eq!(feel.hardness(Tool::Remove), 0.75);
        // A tool that strokes with no brush has Koharu's hard edge, as every
        // tool has by default.
        assert_eq!(feel.hardness(Tool::Marquee), 1.0);
        let koharus = BrushFeel::default();
        for tool in [Tool::Draw, Tool::Eraser, Tool::Remove] {
            assert_eq!(koharus.hardness(tool), 1.0, "{tool:?}");
        }
        assert_eq!(koharus.draw_opacity, 1.0);
    }

    #[test]
    fn the_falloff_runs_from_full_on_the_centre_line_to_nothing_half_a_pixel_out() {
        // At a radius of 10: soft from a pixel in at hardness 0, from 6 at
        // 0.5, and over the last pixel at 0.9, whose falloff can't be less.
        let radius = 10.0;
        for (hardness, falloff_is, ramp) in [
            (0.0, 9.5, [(0.0, 255), (1.0, 255), (6.0, 121), (10.0, 13)]),
            (0.5, 4.5, [(0.0, 255), (6.0, 255), (8.0, 142), (10.0, 28)]),
            (0.9, 1.0, [(0.0, 255), (9.5, 255), (9.9, 153), (10.25, 64)]),
        ] {
            let falloff = falloff(radius, hardness);
            assert_eq!(falloff, falloff_is, "hardness {hardness}");
            for (distance, value) in ramp {
                assert_eq!(
                    covered(distance, radius, falloff),
                    value,
                    "hardness {hardness}, distance {distance}"
                );
            }
            for distance in [10.5, 11.0, 50.0] {
                assert_eq!(covered(distance, radius, falloff), 0, "{distance}");
            }
        }
    }

    /// Booharu 0.0.4's soft stroke through `points`, as its
    /// `cleanup/stroke.rs` drew one into a mask the size of the page, a
    /// byte a page pixel row by row, at a hardness from 0 to 1 rather than
    /// in percent.
    pub(crate) fn reference(
        page: (u32, u32),
        points: &[Point],
        diameter: f64,
        hardness: f64,
    ) -> Vec<u8> {
        let (width, height) = page;
        let mut mask = vec![0u8; (width * height) as usize];
        let radius = diameter * 0.5;
        let falloff = (radius * (1.0 - hardness) - 0.5).max(1.0);
        for (start, end) in points
            .iter()
            .zip(points.iter().skip(1))
            .chain(points.last().map(|point| (point, point)))
        {
            let left = (start.x.min(end.x) - radius - 0.5).floor().max(0.0) as u32;
            let top = (start.y.min(end.y) - radius - 0.5).floor().max(0.0) as u32;
            let right = (start.x.max(end.x) + radius + 0.5)
                .ceil()
                .min(f64::from(width)) as u32;
            let bottom = (start.y.max(end.y) + radius + 0.5)
                .ceil()
                .min(f64::from(height)) as u32;
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let length_squared = dx.mul_add(dx, dy * dy);
            for y in top..bottom {
                for x in left..right {
                    let px = f64::from(x) + 0.5;
                    let py = f64::from(y) + 0.5;
                    let progress = if length_squared <= f64::EPSILON {
                        0.0
                    } else {
                        (((px - start.x) * dx + (py - start.y) * dy) / length_squared)
                            .clamp(0.0, 1.0)
                    };
                    let distance =
                        (px - (start.x + progress * dx)).hypot(py - (start.y + progress * dy));
                    let coverage = (((radius + 0.5 - distance) / falloff).clamp(0.0, 1.0) * 255.0)
                        .round() as u8;
                    let pixel = &mut mask[(y * width + x) as usize];
                    *pixel = (*pixel).max(coverage);
                }
            }
        }
        mask
    }

    /// A page 600 by 300 pixels: three columns of tiles, the last cut to 88
    /// pixels, and two rows, the last cut to 44.
    const PAGE: (u32, u32) = (600, 300);

    fn page() -> Size {
        Size::new(f64::from(PAGE.0), f64::from(PAGE.1))
    }

    /// A Remove stroke's coverage, tinted blue at 0.4, as it previews.
    fn soft(diameter: f64, hardness: f64) -> Soft {
        let blue = Rgba([0.0, 0.0, 1.0, 0.4]);
        Soft::new(Tool::Remove, None, diameter, hardness, page(), blue)
    }

    /// Its coverage over the whole page, a byte a page pixel row by row.
    fn whole(soft: &Soft) -> Vec<u8> {
        let mut page = vec![0u8; (PAGE.0 * PAGE.1) as usize];
        for (&(column, row), tile) in &soft.tiles {
            for (index, &value) in tile.coverage.iter().enumerate() {
                let (x, y) = (index as u32 % tile.width, index as u32 / tile.width);
                page[((row * TILE + y) * PAGE.0 + column * TILE + x) as usize] = value;
            }
        }
        page
    }

    fn points(points: &[(f64, f64)]) -> Vec<Point> {
        points.iter().map(|&point| Point::from(point)).collect()
    }

    /// Strokes across tiles' edges and the page's own, a dot among them.
    fn strokes() -> Vec<Vec<Point>> {
        vec![
            points(&[(40.3, 50.6)]),
            points(&[(200.25, 100.5), (310.75, 120.0)]),
            points(&[(500.0, 230.0), (590.4, 296.2), (620.0, 250.0)]),
            points(&[(10.0, 10.0), (11.5, 11.0), (12.0, 13.25), (300.0, 280.0)]),
        ]
    }

    #[test]
    fn a_strokes_coverage_is_booharu_0_0_4s_at_every_pixel() {
        for diameter in [1.0, 7.3, 20.0, 41.7] {
            for hardness in [0.0, 0.3, 0.5, 0.9, 0.99] {
                for points in strokes() {
                    let mut soft = soft(diameter, hardness);
                    soft.stroke(&points);
                    assert!(
                        whole(&soft) == reference(PAGE, &points, diameter, hardness),
                        "{points:?} at {diameter} and {hardness}"
                    );
                }
            }
        }
    }

    #[test]
    fn segments_added_one_at_a_time_make_what_one_stroke_makes() {
        for points in strokes() {
            let mut grown = soft(20.0, 0.4);
            grown.stroke(&points[..1]);
            for count in 2..=points.len() {
                grown.grow(20.0, &points[..count], count - 1);
            }
            let mut whole_stroke = soft(20.0, 0.4);
            whole_stroke.stroke(&points);
            assert!(whole(&grown) == whole(&whole_stroke), "{points:?}");
            // A move with no new point adds nothing.
            let before = whole(&grown);
            grown.grow(20.0, &points, points.len());
            assert!(whole(&grown) == before, "{points:?}");
        }
    }

    #[test]
    fn another_brush_size_makes_every_segment_again_at_it() {
        let stroke = points(&[(100.0, 100.0), (180.0, 120.0), (260.0, 90.0)]);
        let mut grown = soft(30.0, 0.5);
        grown.stroke(&stroke[..2]);
        grown.grow(12.0, &stroke, 2);
        assert_eq!(grown.diameter, 12.0);
        assert!(whole(&grown) == reference(PAGE, &stroke, 12.0, 0.5));
        // Its tiles shrank with it, so none the wider stroke reached stays.
        let mut narrow = soft(12.0, 0.5);
        narrow.stroke(&stroke);
        let keys = |soft: &Soft| soft.tiles.keys().copied().collect::<Vec<_>>();
        assert_eq!(keys(&grown), keys(&narrow));
    }

    #[test]
    fn only_tiles_the_stroke_reaches_are_kept() {
        let mut soft = soft(20.0, 0.5);
        soft.stroke(&points(&[(100.0, 100.0), (120.0, 100.0)]));
        assert_eq!(soft.tiles.keys().copied().collect::<Vec<_>>(), [(0, 0)]);
        // A segment whose bounds take in the next tiles' first pixels, which
        // it doesn't reach, leaves those tiles out: its coverage ends at the
        // pixel centres half a pixel past its radius, 10.5 pixels out, as
        // theirs are.
        soft.stroke(&points(&[(240.0, 240.0), (246.0, 246.0)]));
        assert_eq!(soft.tiles.keys().copied().collect::<Vec<_>>(), [(0, 0)]);
        // Off the page, nothing.
        soft.stroke(&points(&[(-50.0, -50.0), (900.0, -40.0)]));
        assert_eq!(soft.tiles.len(), 1);
    }

    #[test]
    fn a_tiles_preview_is_its_coverage_in_the_tint_premultiplied() {
        let tint = Rgba([1.0, 0.5, 0.25, 0.8]);
        let mut soft = Soft::new(Tool::Draw, None, 24.0, 0.3, page(), tint);
        soft.stroke(&points(&[(250.0, 250.0), (270.0, 262.0)]));
        soft.refresh();
        assert_eq!(soft.tiles.len(), 4, "the stroke crosses four tiles");
        let [r, g, b] = [255u8, 128, 64];
        for tile in soft.tiles.values() {
            let preview = tile.preview.as_ref().expect("a preview");
            assert_eq!(
                (u32::from(preview.width()), u32::from(preview.height())),
                (tile.width, tile.height)
            );
            for (pixel, &covered) in preview.data().iter().zip(&tile.coverage) {
                // The tint's alpha times the coverage, as Draw paints it,
                // and each channel times that alpha.
                let alpha = (f32::from(covered) * 0.8).round() as u8;
                assert_eq!(pixel.a, alpha);
                let near = |premultiplied: u8, channel: u8| {
                    let exact = f32::from(channel) * f32::from(alpha) / 255.0;
                    (f32::from(premultiplied) - exact).abs() <= 1.0
                };
                assert!(
                    near(pixel.r, r) && near(pixel.g, g) && near(pixel.b, b),
                    "{pixel:?} at {covered}"
                );
            }
        }
        // Full coverage in opaque white shows as it is, and none as nothing.
        let mut white = Soft::new(Tool::Draw, None, 24.0, 0.3, page(), Rgba([1.0; 4]));
        white.stroke(&points(&[(100.0, 100.0)]));
        white.refresh();
        let tile = &white.tiles[&(0, 0)];
        let preview = tile.preview.as_ref().expect("a preview");
        let at = |x: u32, y: u32| preview.data()[(y * tile.width + x) as usize].to_u8_array();
        assert_eq!(at(100, 100), [255; 4]);
        assert_eq!(at(150, 100), [0; 4]);
    }

    #[test]
    fn a_change_makes_only_the_previews_it_changed_again() {
        let mut soft = soft(20.0, 0.5);
        soft.stroke(&points(&[(100.0, 100.0)]));
        soft.stroke(&points(&[(400.0, 200.0)]));
        soft.refresh();
        let previews = |soft: &Soft| -> Vec<Arc<Pixmap>> {
            soft.tiles
                .values()
                .map(|tile| Arc::clone(tile.preview.as_ref().expect("a preview")))
                .collect()
        };
        let before = previews(&soft);
        assert_eq!(before.len(), 2);
        // A segment in the first tile makes its preview again, and only its.
        soft.grow(20.0, &points(&[(100.0, 100.0), (110.0, 100.0)]), 1);
        soft.refresh();
        let after = previews(&soft);
        assert!(!Arc::ptr_eq(&before[0], &after[0]));
        assert!(Arc::ptr_eq(&before[1], &after[1]));
        // A segment over coverage already as full changes nothing.
        soft.grow(
            20.0,
            &points(&[(100.0, 100.0), (110.0, 100.0), (105.0, 100.0)]),
            2,
        );
        soft.refresh();
        let again = previews(&soft);
        assert!(after.iter().zip(&again).all(|(a, b)| Arc::ptr_eq(a, b)));
        // Another tint makes every one again.
        soft.tinted(Rgba([1.0, 0.0, 0.0, 1.0]));
        soft.refresh();
        let tinted = previews(&soft);
        assert!(again.iter().zip(&tinted).all(|(a, b)| !Arc::ptr_eq(a, b)));
        // The second tile, from page pixel (256, 0), shows the dot at
        // (400, 200) in the new tint.
        let at = 200 * 256 + (400 - 256);
        assert_eq!(tinted[1].data()[at].to_u8_array(), [255, 0, 0, 255]);
    }

    /// How long one segment takes at brush sizes from 50 to 2000 pixels,
    /// at hardness 0.5 on a 6688 by 9600 page, for its coverage and then
    /// the previews it changed, to read rather than check: run it with
    /// `cargo test -p koharu-canvas -- --ignored --nocapture timing`.
    #[test]
    #[ignore = "a timing, not a check"]
    fn timing_one_segment() {
        let page = Size::new(6688.0, 9600.0);
        let tint = Rgba([0.0, 0.0, 1.0, 0.4]);
        for diameter in [50.0, 418.0, 2000.0] {
            for length in [4.0, 100.0] {
                let mut soft = Soft::new(Tool::Remove, None, diameter, 0.5, page, tint);
                let start = Point::new(3000.0, 4000.0);
                soft.stroke(&[start]);
                soft.refresh();
                let end = start + Vec2::new(length * 0.8, length * 0.6);
                let started = Instant::now();
                soft.grow(diameter, &[start, end], 1);
                let covered = started.elapsed();
                soft.refresh();
                let previewed = started.elapsed() - covered;
                eprintln!(
                    "diameter {diameter}, segment {length} px: coverage {covered:?}, previews \
                     {previewed:?}, tiles {}",
                    soft.tiles.len()
                );
            }
        }
    }
}
