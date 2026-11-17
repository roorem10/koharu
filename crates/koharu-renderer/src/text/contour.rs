//! Booharu's Contour borders: a text's letters merged into one shape and
//! grown outward with round corners, filled as regions under every fill.
//! Koharu's own borders, the Expansion method, grow each glyph's outline on
//! its own with `kurbo::expand_path`, which grows a whole outline by one sign,
//! so a dot or counter wound the other way shrinks instead and loses its
//! border (koharu-rs #1073, #1292). Merging the letters first gives every
//! dot, counter, overlap, and pair of touching letters the same border.

use i_overlay::{
    core::{fill_rule::FillRule, overlay_rule::OverlayRule},
    float::{simplify::SimplifyShape, single::SingleFloatOverlay},
    mesh::float::{
        outline::offset::OutlineOffset,
        style::{LineJoin, OutlineStyle},
    },
};
use koharu_scene::Color;
use kurbo::{Affine, BezPath, Diagonal2, Join, PathEl, Shape as _};
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
        if run.fill.components[3] < 1.0 {
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
        group.opaque &= run.fill.components[3] >= 1.0;
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
/// fill draws them: grown by a synthetic bold and slanted or turned as
/// `draw` draws them, each wound the same way, so letters that overlap merge
/// rather than cancel.
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
            // glifo's synthetic bold, as the fill draws it.
            path = kurbo::expand_path(&path, Diagonal2::new(bold, bold), Join::Miter, 4.0, 0.1);
        }
        path.apply_affine(Affine::translate((f64::from(glyph.x), f64::from(glyph.y))) * turn);
        if path.area() < 0.0 {
            path = path.reverse_subpaths();
        }
        letters.extend(path.elements().iter().copied());
    }
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
        let contour = laid(None)?;
        let regions = contour.contours.as_ref().expect("Contour is the default");
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
        let expansion = laid(Some(StrokeMethod::Expansion))?;
        assert!(expansion.contours.is_none());
        assert_eq!(
            expansion.resolved.runs[0]
                .outer_stroke
                .map(|(_, width)| width),
            Some(3.0)
        );
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
                ..TextStyle::default()
            },
            ..Text::default()
        };
        let layout = crate::text::layout(&text, |_, _| Ok(vec![font.clone()]))?.expect("a layout");
        let regions = layout.contours.as_ref().expect("Contour is the default");
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
        assert!(regions[0].1.contains(Point::new((bordered.x1 + clear.x0) / 2.0, middle)));
        assert!(!regions[0].1.contains(Point::new(clear.x0 + 3.0, middle)));
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
