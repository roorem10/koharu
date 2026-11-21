//! Booharu's Contour borders: a text's letters merged into one shape and
//! grown outward with round corners, filled as regions under every fill.
//! Koharu's own borders, the Expansion method and the default, stroke every
//! run's glyphs under every fill. Koharu 0.89.0 grows them, and a synthetic
//! bold, by a stroke in place of `kurbo::expand_path`, which grew a whole
//! outline one way, so a dot wound against the rest of its letter shrank and
//! lost its border (koharu-rs #1073, #1292); now every contour keeps one, and
//! Contour grows a synthetic bold the same way. Contour stays a choice beside
//! them: it grows one border around the letters merged, every run with that
//! border together, filled once, so a translucent border shows no darker
//! overlap where letters touch, even across runs. Its regions leave out every
//! letter whose fill isn't opaque, bordered or not, and such a fill's own
//! border is only the ring outside its letters, so no border shows through a
//! letter, as a stroke under the fill does.

use i_overlay::{
    core::{fill_rule::FillRule, overlay_rule::OverlayRule},
    float::{simplify::SimplifyShape, single::SingleFloatOverlay},
    mesh::float::{
        outline::offset::OutlineOffset,
        style::{LineJoin, OutlineStyle},
    },
};
use koharu_scene::Color;
use kurbo::{Affine, BezPath, Join, PathEl, Shape as _, Stroke, StrokeOpts};
use skrifa::{
    GlyphId, MetadataProvider,
    instance::{LocationRef, NormalizedCoord, Size},
    outline::{DrawSettings, OutlinePen},
};

use super::{TextRun, arrange::GlyphRun, draw};

/// A region as i_overlay holds it: shapes, each an outer contour and its
/// holes.
type Shapes = Vec<Vec<Vec<[f64; 2]>>>;

/// How far a curve may stray when the letters are flattened, in the text's
/// own pixels: a tenth of a pixel at four times zoom. The fills keep their
/// curves.
const FLATTEN: f64 = 0.02;

/// The runs that share a border, merged into one shape.
struct Group {
    stroke: (Color, f32),
    outer: Option<(Color, f32)>,
    /// Every run's fill is opaque, so it hides what lies under its letters.
    opaque: bool,
    letters: BezPath,
}

/// The borders of a text's glyph `runs`, as `resolved` paints them: each a
/// colour and the region it fills, in the order they draw under every fill,
/// every second border before every first one.
pub(super) fn regions(runs: &[GlyphRun], resolved: &[TextRun]) -> Vec<(Color, BezPath)> {
    let mut groups: Vec<Group> = Vec::new();
    // Letters with a translucent fill, bordered or not, which no border may
    // show through.
    let mut clear = BezPath::new();
    for (glyphs, run) in runs.iter().zip(resolved) {
        // Booharu: a gradient fill is translucent where any stop is.
        if !crate::booharu::opaque(run) {
            outlines(glyphs, run.size, &mut clear);
        }
        let Some(stroke) = run.stroke else {
            continue;
        };
        let at = match groups
            .iter()
            .position(|group| group.stroke == stroke && group.outer == run.outer_stroke)
        {
            Some(at) => at,
            None => {
                groups.push(Group {
                    stroke,
                    outer: run.outer_stroke,
                    opaque: true,
                    letters: BezPath::new(),
                });
                groups.len() - 1
            }
        };
        let group = &mut groups[at];
        group.opaque &= crate::booharu::opaque(run); // Booharu: every stop too.
        outlines(glyphs, run.size, &mut group.letters);
    }
    let clear = merged(&clear);
    let cut = |region: Shapes| match clear.is_empty() {
        true => region,
        false => region.overlay(&clear, OverlayRule::Difference, FillRule::NonZero),
    };
    let mut outer = Vec::new();
    let mut first = Vec::new();
    for group in groups {
        let letters = merged(&group.letters);
        if letters.is_empty() {
            continue;
        }
        let (color, width) = group.stroke;
        let grown = offset(&letters, f64::from(width));
        if let Some((outer_color, outer_width)) = group.outer {
            // The first border's region grown on, the same shape as the
            // letters grown by both widths at once, at less cost.
            let reach = offset(&grown, f64::from(outer_width));
            // Whole under an opaque border and fill, which cover its inside
            // without a seam between the two; otherwise only its ring, so it
            // doesn't show through them.
            let region = match group.opaque && color.components[3] >= 1.0 {
                true => reach,
                false => reach.overlay(&grown, OverlayRule::Difference, FillRule::NonZero),
            };
            outer.push((outer_color, path(&cut(region))));
        }
        let region = match group.opaque {
            true => grown,
            false => grown.overlay(&letters, OverlayRule::Difference, FillRule::NonZero),
        };
        first.push((color, path(&cut(region))));
    }
    outer.extend(first);
    outer
}

/// Adds the outlines of `run`'s glyphs at `size` to `letters`, where the
/// fill draws them: grown by a synthetic bold ([`emboldened`]) and slanted or
/// turned as `draw` draws them, each wound the same way, so letters that
/// overlap merge rather than cancel.
fn outlines(run: &GlyphRun, size: f32, letters: &mut BezPath) {
    let font = &run.font;
    let Ok(face) = skrifa::FontRef::from_index(font.data.data.as_ref(), font.data.index) else {
        return;
    };
    let coords: Vec<NormalizedCoord> = font
        .coords
        .iter()
        .map(|bits| NormalizedCoord::from_bits(*bits))
        .collect();
    let outlines = face.outline_glyphs();
    let bold = draw::bold(font, size);
    // As glifo places an outline: the run's glyph transform after flipping
    // it from the font's y up.
    let turn = draw::turn(font, run.sideways).unwrap_or(Affine::IDENTITY) * Affine::FLIP_Y;
    for glyph in &run.glyphs {
        let Some(outline) = outlines.get(GlyphId::new(glyph.id)) else {
            continue;
        };
        let mut pen = Pen(BezPath::new());
        let settings = DrawSettings::unhinted(Size::new(size), LocationRef::new(&coords));
        if outline.draw(settings, &mut pen).is_err() {
            continue;
        }
        let mut path = pen.0;
        if bold > 0.0 {
            path = emboldened(&path, bold);
        }
        path.apply_affine(Affine::translate((f64::from(glyph.x), f64::from(glyph.y))) * turn);
        if path.area() < 0.0 {
            path = path.reverse_subpaths();
        }
        letters.extend(path.elements().iter().copied());
    }
}

/// `outline` grown by a synthetic `bold` as the fill draws it (`draw`): the
/// outline together with its stroke twice as wide, with mitred joins, so
/// every contour grows by `bold`, whichever way it's wound.
fn emboldened(outline: &BezPath, bold: f64) -> BezPath {
    let stroke = Stroke::new(2.0 * bold).with_join(Join::Miter);
    let stroked = kurbo::stroke(
        outline.elements().iter().copied(),
        &stroke,
        &StrokeOpts::default(),
        FLATTEN,
    );
    path(&merged(outline).overlay(&merged(&stroked), OverlayRule::Union, FillRule::NonZero))
}

/// `letters` merged into one shape: flattened, since the boolean operations
/// take polygons, and filled by the non-zero rule, which keeps counters and
/// fills an overlap.
fn merged(letters: &BezPath) -> Shapes {
    let mut contours: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut contour = Vec::new();
    let mut close = |contour: &mut Vec<[f64; 2]>| {
        if contour.len() >= 3 {
            contours.push(std::mem::take(contour));
        }
        contour.clear();
    };
    kurbo::flatten(letters.elements().iter().copied(), FLATTEN, |element| {
        match element {
            PathEl::MoveTo(point) => {
                close(&mut contour);
                contour.push([point.x, point.y]);
            }
            PathEl::LineTo(point) => contour.push([point.x, point.y]),
            PathEl::ClosePath => close(&mut contour),
            // Flattening leaves only lines.
            PathEl::QuadTo(..) | PathEl::CurveTo(..) => {}
        }
    });
    close(&mut contour);
    contours.simplify_shape(FillRule::NonZero)
}

/// `shapes` grown outward by `distance` with round corners, each arc within
/// a hundredth of a pixel of a circle.
fn offset(shapes: &Shapes, distance: f64) -> Shapes {
    // i_overlay takes an arc's step as its segment length over its radius.
    let step = (8.0 * 0.01 / distance).sqrt();
    shapes.outline(&OutlineStyle::new(distance).line_join(LineJoin::Round(step)))
}

/// `shapes` as a path, filled by the non-zero rule.
fn path(shapes: &Shapes) -> BezPath {
    let mut path = BezPath::new();
    for contour in shapes.iter().flatten() {
        let Some((first, rest)) = contour.split_first() else {
            continue;
        };
        path.move_to((first[0], first[1]));
        for point in rest {
            path.line_to((point[0], point[1]));
        }
        path.close_path();
    }
    path
}

struct Pen(BezPath);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((f64::from(x), f64::from(y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((f64::from(x), f64::from(y)));
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0
            .quad_to((f64::from(cx), f64::from(cy)), (f64::from(x), f64::from(y)));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to(
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(cx1), f64::from(cy1)),
            (f64::from(x), f64::from(y)),
        );
    }

    fn close(&mut self) {
        self.0.close_path();
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use koharu_scene::{CharStyle, Rgba, StrokeMethod, Text, TextStyle};
    use kurbo::{Circle, Point, Rect};

    use super::*;
    use crate::fonts::Library;

    #[test]
    fn a_layout_grows_second_borders_then_first_ones_past_its_lines() -> Result<()> {
        let mut library = Library::new();
        let font = ["Arial", "DejaVu Sans", "Helvetica"]
            .into_iter()
            .find_map(|family| library.family(family).ok())
            .expect("an installed sans-serif");
        let laid = |method| -> Result<crate::text::TextLayout> {
            let text = Text {
                content: "i!".into(),
                style: TextStyle {
                    chars: CharStyle {
                        stroke: Some(Rgba([1.0, 1.0, 1.0, 1.0])),
                        stroke_width: Some(2.0),
                        outer_stroke: Some(Rgba([1.0, 0.0, 0.0, 1.0])),
                        outer_stroke_width: Some(3.0),
                        ..CharStyle::default()
                    },
                    stroke_method: method,
                    ..TextStyle::default()
                },
                ..Text::default()
            };
            Ok(crate::text::layout(&text, |_, _| Ok(vec![font.clone()]))?.expect("a layout"))
        };
        let contour = laid(Some(StrokeMethod::Contour))?;
        let regions = contour.contours.as_ref().expect("Contour's regions");
        let colors: Vec<_> = regions.iter().map(|(color, _)| color.components).collect();
        assert_eq!(colors, [[1.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]]);
        // The second border holds the first, which holds the letters.
        let (outer, first) = (regions[0].1.bounding_box(), regions[1].1.bounding_box());
        assert!(outer.contains_rect(first.inflate(2.9, 2.9)));
        assert!(contour.bounds.contains_rect(outer));
        // The first border hugs the letters where the fill draws them: by the
        // points their outlines pass through, a little past their ink.
        let run = &contour.runs[0];
        let size = contour.resolved.runs[0].size;
        let ink = run
            .glyphs
            .iter()
            .filter_map(|glyph| {
                let ink = run.font.ink(glyph.id, size)?;
                let at = kurbo::Vec2::new(f64::from(glyph.x), f64::from(glyph.y));
                Some(Rect::new(ink.x0, -ink.y1, ink.x1, -ink.y0) + at)
            })
            .reduce(|all, ink| all.union(ink))
            .expect("the letters ink");
        assert!(
            first.contains_rect(ink.inflate(1.5, 1.5)),
            "{first:?} {ink:?}"
        );
        assert!(
            ink.inflate(2.5, 2.5).contains_rect(first),
            "{first:?} {ink:?}"
        );
        // No method is Koharu's border, as Expansion: the glyphs' own strokes,
        // no regions.
        for method in [None, Some(StrokeMethod::Expansion)] {
            let expansion = laid(method)?;
            assert!(expansion.contours.is_none(), "{method:?}");
            assert_eq!(
                expansion.resolved.runs[0]
                    .outer_stroke
                    .map(|(_, width)| width),
                Some(3.0)
            );
        }
        Ok(())
    }

    #[test]
    fn no_border_shows_through_a_translucent_letter_beside_it() -> Result<()> {
        let mut library = Library::new();
        let font = ["Arial", "DejaVu Sans", "Helvetica"]
            .into_iter()
            .find_map(|family| library.family(family).ok())
            .expect("an installed sans-serif");
        // A bordered O, then an unbordered one at half opacity, whose letter
        // the first one's wide border reaches under.
        let text = Text {
            content: koharu_scene::RichText(vec![
                koharu_scene::Span {
                    insert: "O".into(),
                    attributes: CharStyle {
                        stroke: Some(Rgba([1.0, 0.0, 0.0, 1.0])),
                        stroke_width: Some(20.0),
                        ..CharStyle::default()
                    },
                },
                koharu_scene::Span {
                    insert: "O".into(),
                    attributes: CharStyle {
                        fill: Some(Rgba([0.0, 0.0, 0.0, 0.5])),
                        ..CharStyle::default()
                    },
                },
            ]),
            style: TextStyle {
                chars: CharStyle {
                    size: Some(100.0),
                    ..CharStyle::default()
                },
                stroke_method: Some(StrokeMethod::Contour),
                ..TextStyle::default()
            },
            ..Text::default()
        };
        let layout = crate::text::layout(&text, |_, _| Ok(vec![font.clone()]))?.expect("a layout");
        let regions = layout.contours.as_ref().expect("Contour's regions");
        assert_eq!(regions.len(), 1);
        let ink: Vec<Rect> = layout
            .runs
            .iter()
            .zip(&layout.resolved.runs)
            .flat_map(|(run, resolved)| {
                run.glyphs.iter().filter_map(move |glyph| {
                    let ink = run.font.ink(glyph.id, resolved.size)?;
                    let at = kurbo::Vec2::new(f64::from(glyph.x), f64::from(glyph.y));
                    Some(Rect::new(ink.x0, -ink.y1, ink.x1, -ink.y0) + at)
                })
            })
            .collect();
        let [bordered, clear] = ink[..] else {
            panic!("two letters: {ink:?}")
        };
        let middle = bordered.center().y;
        // The border reaches across the gap and under the second O's left
        // stroke, but leaves that stroke clear.
        assert!(clear.x0 - bordered.x1 < 15.0, "{bordered:?} {clear:?}");
        let (_, region) = &regions[0];
        assert!(region.contains(Point::new((bordered.x1 + clear.x0) / 2.0, middle)));
        assert!(!region.contains(Point::new(clear.x0 + 3.0, middle)));
        Ok(())
    }

    /// A square with a dot inside its hole, the dot wound like the hole, as a
    /// font may draw the dot of an `i` or `!` against its stem.
    fn square_with_a_reversed_dot() -> BezPath {
        let mut letters = BezPath::new();
        letters.extend(Rect::new(0.0, 0.0, 30.0, 30.0).path_elements(0.1));
        letters.extend(
            Rect::new(5.0, 5.0, 25.0, 25.0)
                .path_elements(0.1)
                .collect::<BezPath>()
                .reverse_subpaths(),
        );
        letters.extend(
            Circle::new((15.0, 15.0), 2.0)
                .path_elements(0.1)
                .collect::<BezPath>()
                .reverse_subpaths(),
        );
        letters
    }

    #[test]
    fn a_dot_wound_like_a_counter_still_grows() {
        let letters = merged(&square_with_a_reversed_dot());
        let grown = path(&offset(&letters, 1.5));
        // The dot grows from a radius of 2 to 3.5, where Koharu's single sign
        // shrinks it; the hole shrinks by the same 1.5 and the square grows.
        assert!(grown.contains(Point::new(15.0 + 3.0, 15.0)));
        assert!(!grown.contains(Point::new(15.0 + 4.5, 15.0)));
        assert!(grown.contains(Point::new(6.0, 15.0)));
        assert!(!grown.contains(Point::new(8.0, 15.0)));
        assert!(grown.contains(Point::new(-1.0, 15.0)));
        assert!(!grown.contains(Point::new(-2.0, 15.0)));
    }

    #[test]
    fn a_synthetic_bold_grows_a_dot_wound_against_the_rest() {
        // kurbo's `expand_path` grew a whole outline one way, by its area, so
        // this dot, wound like the hole, shrank.
        let grown = emboldened(&square_with_a_reversed_dot(), 1.0);
        // The dot grows from a radius of 2 to 3, the hole shrinks by 1 and the
        // square grows by 1.
        assert!(grown.contains(Point::new(15.0 + 2.8, 15.0)));
        assert!(!grown.contains(Point::new(15.0 + 3.2, 15.0)));
        assert!(grown.contains(Point::new(5.8, 15.0)));
        assert!(!grown.contains(Point::new(6.2, 15.0)));
        assert!(grown.contains(Point::new(-0.8, 15.0)));
        assert!(!grown.contains(Point::new(-1.2, 15.0)));
    }

    #[test]
    fn a_synthetic_bold_holds_its_outline_and_mitres_its_corners() {
        let outline = square_with_a_reversed_dot();
        let grown = emboldened(&outline, 1.0);
        // Nothing of the outline lies outside it.
        let outside =
            merged(&outline).overlay(&merged(&grown), OverlayRule::Difference, FillRule::NonZero);
        assert!(outside.is_empty(), "{outside:?}");
        // A corner grows to a point, as the fill's mitred stroke draws it,
        // and nothing reaches further than the bold.
        assert!(grown.contains(Point::new(-0.9, -0.9)));
        let reach = outline.bounding_box().inflate(1.01, 1.01);
        assert!(reach.contains_rect(grown.bounding_box()), "{reach:?}");
    }

    /// `layout` drawn as the canvas and the export draw a text, with its
    /// borders while `strokes` says so, on a transparent drawing its bounds'
    /// size; `None` without a GPU.
    fn drawn(layout: &crate::text::TextLayout, strokes: bool) -> Result<Option<image::RgbaImage>> {
        let Ok(gpu) = crate::Gpu::get() else {
            eprintln!("skipped drawing without a GPU");
            return Ok(None);
        };
        let size = layout.bounds.size();
        let mut drawing = gpu.drawing(size.width.ceil() as u16, size.height.ceil() as u16);
        crate::text::draw(
            &mut drawing,
            layout,
            Affine::translate((-layout.bounds.x0, -layout.bounds.y0)),
            strokes,
        )?;
        Ok(Some(drawing.read()?))
    }

    /// How many of `image`'s pixels are opaque and near `color`.
    fn count(image: &image::RgbaImage, color: [u8; 3]) -> usize {
        image
            .pixels()
            .filter(|pixel| {
                pixel.0[3] > 200 && (0..3).all(|at| pixel.0[at].abs_diff(color[at]) < 60)
            })
            .count()
    }

    #[test]
    fn contour_borders_draw_second_border_then_first_under_the_fill() -> Result<()> {
        let mut library = Library::new();
        let font = ["Arial", "DejaVu Sans", "Helvetica"]
            .into_iter()
            .find_map(|family| library.family(family).ok())
            .expect("an installed sans-serif");
        let text = Text {
            content: "Stroked".into(),
            style: TextStyle {
                chars: CharStyle {
                    size: Some(48.0),
                    stroke: Some(Rgba([1.0, 0.0, 0.0, 1.0])),
                    stroke_width: Some(3.0),
                    outer_stroke: Some(Rgba([0.0, 0.0, 1.0, 1.0])),
                    outer_stroke_width: Some(3.0),
                    ..CharStyle::default()
                },
                stroke_method: Some(StrokeMethod::Contour),
                ..TextStyle::default()
            },
            ..Text::default()
        };
        let layout = crate::text::layout(&text, |_, _| Ok(vec![font.clone()]))?.expect("a layout");
        assert!(layout.contours.is_some());
        let Some(image) = drawn(&layout, true)? else {
            return Ok(());
        };
        // Each region shows where the one over it doesn't reach: the blue
        // second border round the red first one, round the black letters.
        let (blue, red, black) = (
            count(&image, [0, 0, 255]),
            count(&image, [255, 0, 0]),
            count(&image, [0, 0, 0]),
        );
        assert!(
            blue > 100 && red > 100 && black > 100,
            "{blue} {red} {black}"
        );
        // Without strokes, the letters alone.
        let image = drawn(&layout, false)?.expect("a GPU");
        assert_eq!(count(&image, [0, 0, 255]) + count(&image, [255, 0, 0]), 0);
        assert!(count(&image, [0, 0, 0]) > 100);
        Ok(())
    }

    #[test]
    fn no_contour_border_shows_through_the_see_through_letter_it_rings() -> Result<()> {
        let mut library = Library::new();
        let font = ["Arial", "DejaVu Sans", "Helvetica"]
            .into_iter()
            .find_map(|family| library.family(family).ok())
            .expect("an installed sans-serif");
        // An I at half opacity, its border wider than half its stem.
        let laid = |method| -> Result<crate::text::TextLayout> {
            let text = Text {
                content: "I".into(),
                style: TextStyle {
                    chars: CharStyle {
                        size: Some(100.0),
                        fill: Some(Rgba([0.0, 0.0, 0.0, 0.5])),
                        stroke: Some(Rgba([1.0, 0.0, 0.0, 1.0])),
                        stroke_width: Some(10.0),
                        ..CharStyle::default()
                    },
                    stroke_method: method,
                    ..TextStyle::default()
                },
                ..Text::default()
            };
            Ok(crate::text::layout(&text, |_, _| Ok(vec![font.clone()]))?.expect("a layout"))
        };
        // The middle of the I's ink, where the drawing puts it.
        let middle = |layout: &crate::text::TextLayout| -> (u32, u32) {
            let (run, glyph) = (&layout.runs[0], &layout.runs[0].glyphs[0]);
            let ink = run
                .font
                .ink(glyph.id, layout.resolved.runs[0].size)
                .expect("the I's ink");
            let at = kurbo::Vec2::new(f64::from(glyph.x), f64::from(glyph.y));
            let middle = Rect::new(ink.x0, -ink.y1, ink.x1, -ink.y0).center() + at;
            (
                (middle.x - layout.bounds.x0) as u32,
                (middle.y - layout.bounds.y0) as u32,
            )
        };
        let contour = laid(Some(StrokeMethod::Contour))?;
        let Some(image) = drawn(&contour, true)? else {
            return Ok(());
        };
        let (x, y) = middle(&contour);
        let pixel = image.get_pixel(x, y).0;
        // Half see-through black, with no red under it, ringed by the border.
        assert!(pixel[0] < 16 && pixel[3].abs_diff(128) < 16, "{pixel:?}");
        assert!(count(&image, [255, 0, 0]) > 100);
        // Koharu's border reaches that far in, under the whole letter, so it
        // shows through there.
        let expansion = laid(None)?;
        let image = drawn(&expansion, true)?.expect("a GPU");
        let (x, y) = middle(&expansion);
        let pixel = image.get_pixel(x, y).0;
        assert!(pixel[0] > 100 && pixel[3] > 250, "{pixel:?}");
        Ok(())
    }

    #[test]
    fn corners_round_off_at_the_border_width() {
        let letters = merged(&Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.1));
        let grown = path(&offset(&letters, 4.0));
        // Straight along an edge, round past a corner: (-2.9, -2.9) is 4.1
        // from the corner.
        assert!(grown.contains(Point::new(5.0, -3.9)));
        assert!(grown.contains(Point::new(-2.8, -2.8)));
        assert!(!grown.contains(Point::new(-2.9, -2.9)));
    }
}
