//! Booharu's text borders as regions, filled under every fill: Contour's, a
//! text's letters merged into one shape and grown outward with round corners,
//! and Koharu's own, the Expansion method and the default, each run's glyphs
//! grown as Koharu's border pass grows them.
//!
//! Koharu 0.89.0 grows its borders, and a synthetic bold, by stroking every
//! run's glyphs under every fill, in place of `kurbo::expand_path`, which grew
//! a whole outline one way, so a dot wound against the rest of its letter
//! shrank and lost its border (koharu-rs #1073, #1292); now every contour
//! keeps one. But where the stroke's half-width passes a contour's own size,
//! as beside the dot of an i or a colon once the border is wider than about a
//! fifth of the text size, the stroke's inner side folds back past the
//! contour, and filled by the non-zero rule the folds cancel it, leaving holes
//! in the border beside the dot, as Koharu 0.89.0 draws it too. So Expansion's
//! border passes fill the region the glyphs and their stroke cover where the
//! stroke doesn't fold: each run's glyphs, as glifo strokes them, grown by the
//! pass's growth with round joins ([`grown`]), made once as the text is laid
//! out. It looks as Koharu draws it everywhere else, and the fill pass is
//! still Koharu's own.
//!
//! Contour stays a choice beside it: it grows one border around the letters
//! merged, every run with that border together, filled once, so a translucent
//! border shows no darker overlap where letters touch, even across runs, and
//! grows a synthetic bold as Koharu's fill does. Its regions leave out every
//! letter whose fill isn't opaque, bordered or not, and such a fill's own
//! border is only the ring outside its letters, so no border shows through a
//! letter, as a border under the fill does.

use std::collections::HashMap;

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
use vello_gpu::peniko::Fill;

use super::{TextRun, arrange::GlyphRun, draw};
use crate::Drawing;

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
            outlines(glyphs, run.size, Outlines::Filled, |_, path| {
                clear.extend(path)
            });
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
        outlines(glyphs, run.size, Outlines::Filled, |_, path| {
            group.letters.extend(path);
        });
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

/// A run's border by the Expansion method, Koharu's and the default, as the
/// regions `draw`'s border passes fill: its glyphs grown by what each pass
/// grows them by, in the text's own coordinates. `None` where the run has no
/// such border or the pass grows it by nothing, which `draw` draws as Koharu
/// does.
#[derive(Default)]
pub(crate) struct Grown {
    /// Grown by the second border's width too (`Pass::Outer`).
    pub(crate) outer: Option<BezPath>,
    /// Grown by the border's width (`Pass::Stroke`).
    pub(crate) stroke: Option<BezPath>,
}

/// The borders of a text's glyph `runs`, as `resolved` paints them, by the
/// Expansion method: for each run, its glyphs as Koharu's border pass strokes
/// them ([`Outlines::Stroked`]), each merged by the non-zero rule and grown
/// with round joins by that pass's growth, the border's width and its
/// synthetic bold's growth (`draw::bold`), and the second border's width as
/// well for the second border. That's every point within the growth of the
/// glyphs, which the pass's fill and its stroke twice as wide cover together
/// wherever the stroke doesn't fold, with no hole beside a dot or in a small
/// counter. Empty when no run has a border.
///
/// Each glyph grows alone, once for every glyph it repeats, and the run's
/// region holds them all where the run sets them, overlapping, which the
/// non-zero rule fills as one: the glyphs merged and grown together are the
/// same shape, but growing them so costs many times more, as a long run's
/// curves fold over each other far more often than one glyph's do.
pub(super) fn grown(runs: &[GlyphRun], resolved: &[TextRun]) -> Vec<Grown> {
    if resolved.iter().all(|run| run.stroke.is_none()) {
        return Vec::new();
    }
    runs.iter()
        .zip(resolved)
        .map(|(glyphs, run)| {
            let Some((_, width)) = run.stroke else {
                return Grown::default();
            };
            // What `draw` grows each pass by, reckoned as it does.
            let bold = draw::bold(&glyphs.font, run.size);
            let stroke = bold + f64::from(width);
            if stroke <= 0.0 {
                return Grown::default();
            }
            // How much further the second border reaches, which a run has
            // only beside a border of its own (`SpanStyle::new`).
            let more = run.outer_stroke.map(|(_, outer)| f64::from(outer));
            let turn = draw::turn(&glyphs.font, glyphs.sideways).unwrap_or(Affine::IDENTITY);
            let mut alike: HashMap<u32, [Option<BezPath>; 2]> = HashMap::new();
            let mut regions = [BezPath::new(), BezPath::new()];
            outlines(glyphs, run.size, Outlines::Stroked, |glyph, outline| {
                let grown = alike.entry(glyph.id).or_insert_with(|| {
                    let first = grow(&merged(&outline), stroke);
                    // The second border grows on from the first, the same
                    // shape as the letter grown by both at once, at less cost.
                    let second = more.map(|more| grow(&first, more));
                    // Turned once grown, as glifo strokes a glyph before its
                    // glyph transform (see `Outlines::Stroked`).
                    [second, Some(first)].map(|grown| {
                        let mut grown = path(&grown?);
                        grown.apply_affine(turn);
                        Some(grown)
                    })
                });
                let at = Affine::translate((f64::from(glyph.x), f64::from(glyph.y)));
                for (region, grown) in regions.iter_mut().zip(grown.iter()) {
                    if let Some(grown) = grown {
                        region.extend(grown.elements().iter().map(|element| at * *element));
                    }
                }
            });
            let [outer, stroke] = regions.map(|region| (!region.is_empty()).then_some(region));
            Grown { outer, stroke }
        })
        .collect()
}

/// `letter` grown outward by `distance` with round corners ([`offset`]),
/// its holes no wider than twice the distance filled in first: every point
/// of such a hole lies within the distance of its side, one way across it
/// or the other, so it fills anyway, and growing it would cost the most, as
/// its sides grown pass over each other.
fn grow(letter: &Shapes, distance: f64) -> Shapes {
    let wide = |hole: &&Vec<[f64; 2]>| {
        let (mut low, mut high) = ([f64::MAX; 2], [f64::MIN; 2]);
        for point in hole.iter() {
            for axis in 0..2 {
                low[axis] = low[axis].min(point[axis]);
                high[axis] = high[axis].max(point[axis]);
            }
        }
        (0..2).all(|axis| high[axis] - low[axis] > 2.0 * distance)
    };
    let letter: Shapes = letter
        .iter()
        .map(|shape| match shape.split_first() {
            Some((outline, holes)) => std::iter::once(outline)
                .chain(holes.iter().filter(wide))
                .cloned()
                .collect(),
            None => Vec::new(),
        })
        .collect();
    offset(&letter, distance)
}

/// Fills `region`, a run's glyphs grown, in `paint` by the non-zero rule,
/// through the drawing's transform, which Koharu's stroke drew through too.
/// One shape, it shows a translucent paint at one even alpha with no layer of
/// its own, which the stroke needed for its overlap with the glyphs. The
/// scene's fill rule is left as it was, for the fills after it.
pub(super) fn fill(drawing: &mut Drawing, region: &BezPath, paint: Color) {
    let state = drawing.scene.save_current_state();
    drawing.scene.set_fill_rule(Fill::NonZero);
    drawing.scene.set_paint(paint);
    drawing.scene.fill_path(region);
    drawing.scene.restore_state(state);
}

/// Which outlines of a run's glyphs [`outlines`] gives.
#[derive(Clone, Copy)]
enum Outlines {
    /// Where the fill draws them, as Contour grows its border from them:
    /// grown by a synthetic bold ([`emboldened`]), then slanted or turned and
    /// placed as `draw` draws them, each wound the same way, so letters that
    /// overlap merge rather than cancel.
    Filled,
    /// As Koharu's border pass strokes them, for the Expansion method's
    /// regions ([`grown`]): not grown by a synthetic bold, which the pass
    /// grows them by with its border, and neither slanted, turned nor placed.
    /// glifo strokes a glyph's outline in its own space, at the run's size,
    /// and only then applies the run's glyph transform: glifo 0.5 keeps a
    /// stroked outline at its font size (`GlyphScaleProperties`), and its
    /// `stroke_uncached_outline_glyph` sets the glyph's transform, its place
    /// after the glyph transform (`draw::turn`) after a flip from the font's
    /// y up, while vello_common 0.4's `flatten::stroke` expands the stroke in
    /// the path's own space before it applies that transform. So the stroke's
    /// round joins slant and turn with the glyph: a synthetic slant shears
    /// them into ellipses, while a quarter turn only moves them. [`grown`]
    /// grows each glyph so, and turns it after. A colour glyph, which glifo
    /// paints in its colours in place of a stroke, is left out.
    Stroked,
}

/// Gives `each` of `run`'s glyphs with its outline at `size`, as `which`
/// gives it, flipped from the font's y up to the text's y down.
fn outlines(
    run: &GlyphRun,
    size: f32,
    which: Outlines,
    mut each: impl FnMut(&glifo::Glyph, BezPath),
) {
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
    let colours = face.color_glyphs();
    let bold = draw::bold(font, size);
    // As glifo places an outline: the run's glyph transform after flipping
    // it from the font's y up.
    let turn = draw::turn(font, run.sideways).unwrap_or(Affine::IDENTITY) * Affine::FLIP_Y;
    for glyph in &run.glyphs {
        let id = GlyphId::new(glyph.id);
        if matches!(which, Outlines::Stroked) && colours.get(id).is_some() {
            continue;
        }
        let Some(outline) = outlines.get(id) else {
            continue;
        };
        let mut pen = Pen(BezPath::new());
        let settings = DrawSettings::unhinted(Size::new(size), LocationRef::new(&coords));
        if outline.draw(settings, &mut pen).is_err() {
            continue;
        }
        let mut path = pen.0;
        match which {
            Outlines::Filled => {
                if bold > 0.0 {
                    path = emboldened(&path, bold);
                }
                path.apply_affine(
                    Affine::translate((f64::from(glyph.x), f64::from(glyph.y))) * turn,
                );
                if path.area() < 0.0 {
                    path = path.reverse_subpaths();
                }
            }
            Outlines::Stroked => path.apply_affine(Affine::FLIP_Y),
        }
        each(glyph, path);
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
    use koharu_scene::{CharStyle, Direction, Rgba, StrokeMethod, Text, TextStyle};
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

    /// The installed sans-serif the tests set text in.
    fn sans() -> crate::fonts::Font {
        let mut library = Library::new();
        ["Arial", "DejaVu Sans", "Helvetica"]
            .into_iter()
            .find_map(|family| library.family(family).ok())
            .expect("an installed sans-serif")
    }

    /// `content` set at 48 px in `font`, in `direction`, filled in `fill`,
    /// with a `border` and maybe a `second` border, each a colour and a
    /// width, by Koharu's method, the default. Its borders are each run's
    /// glyphs grown, which lie inside its bounds.
    fn bordered(
        content: &str,
        font: &crate::fonts::Font,
        direction: Direction,
        fill: Rgba,
        border: (Rgba, f64),
        second: Option<(Rgba, f64)>,
    ) -> Result<crate::text::TextLayout> {
        let text = Text {
            content: content.into(),
            direction,
            style: TextStyle {
                chars: CharStyle {
                    size: Some(48.0),
                    fill: Some(fill),
                    stroke: Some(border.0),
                    stroke_width: Some(border.1),
                    outer_stroke: second.map(|(color, _)| color),
                    outer_stroke_width: second.map(|(_, width)| width),
                    ..CharStyle::default()
                },
                ..TextStyle::default()
            },
            ..Text::default()
        };
        let layout = crate::text::layout(&text, |_, _| Ok(vec![font.clone()]))?.expect("a layout");
        assert!(layout.contours.is_none());
        assert_eq!(layout.grown.len(), layout.runs.len());
        for grown in &layout.grown {
            assert!(grown.stroke.is_some());
            assert_eq!(grown.outer.is_some(), second.is_some());
            for region in [&grown.outer, &grown.stroke].into_iter().flatten() {
                let reach = region.bounding_box();
                assert!(
                    layout.bounds.contains_rect(reach),
                    "{reach:?} past {:?}",
                    layout.bounds
                );
            }
        }
        Ok(layout)
    }

    /// The pixels of `image` within `reach` of its ink, the pixels it covers
    /// at least half, from one pixel's centre to another's.
    fn near_ink(image: &image::RgbaImage, reach: f64) -> Vec<(u32, u32)> {
        let (width, height) = (i64::from(image.width()), i64::from(image.height()));
        let mut near = vec![false; (width * height) as usize];
        let span = reach.floor() as i64;
        for (x, y, pixel) in image.enumerate_pixels() {
            if pixel.0[3] < 128 {
                continue;
            }
            for dy in -span..=span {
                for dx in -span..=span {
                    let (x, y) = (i64::from(x) + dx, i64::from(y) + dy);
                    let inside = (0..width).contains(&x) && (0..height).contains(&y);
                    if inside && ((dx * dx + dy * dy) as f64) <= reach * reach {
                        near[(y * width + x) as usize] = true;
                    }
                }
            }
        }
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .filter(|(x, y)| near[(y * width + x) as usize])
            .map(|(x, y)| (x as u32, y as u32))
            .collect()
    }

    #[test]
    fn a_wide_border_leaves_no_hole_beside_a_dot() -> Result<()> {
        let font = sans();
        let (white, clear) = (Rgba([1.0, 1.0, 1.0, 1.0]), Rgba([1.0, 1.0, 1.0, 0.0]));
        let (red, black) = (Rgba([1.0, 0.0, 0.0, 1.0]), Rgba([0.0, 0.0, 0.0, 1.0]));
        // Borders reaching a quarter of the size, past where Koharu's stroke
        // folds beside a dot and leaves a hole there (about a fifth): one
        // border, then a second border under a narrow one, opaque, and clear
        // so that the second border's own holes show through it.
        let borders = [
            ((black, 12.0), None),
            ((red, 4.0), Some((black, 8.0))),
            ((clear, 4.0), Some((black, 8.0))),
        ];
        for (border, second) in borders {
            let content = ": … i! j? ;";
            let layout = bordered(content, &font, Direction::Horizontal, white, border, second)?;
            let Some(image) = drawn(&layout, true)? else {
                return Ok(());
            };
            let letters = drawn(&layout, false)?.expect("a GPU");
            // Every pixel within the borders' reach of the letters' own ink,
            // short of a pixel and a half for the edges' antialiasing, is
            // opaque. Koharu's stroke leaves 430 such pixels clear with one
            // border, and 6 and 430 with the second.
            let reach = border.1 + second.map_or(0.0, |(_, width)| width) - 1.5;
            let near = near_ink(&letters, reach);
            let holes: Vec<_> = near
                .iter()
                .filter(|(x, y)| image.get_pixel(*x, *y).0[3] < 250)
                .collect();
            assert!(
                holes.is_empty(),
                "{} of {} pixels show through, as at {:?}",
                holes.len(),
                near.len(),
                &holes[..holes.len().min(8)]
            );
        }
        Ok(())
    }

    /// `layout`'s outermost borders as Koharu 0.89.0 draws them, in black, on
    /// a drawing as `drawn`'s: each run's glyphs filled, then stroked twice as
    /// wide as the pass grows them, with round joins, through
    /// `Drawing::glyphs`, by its second border's pass (`Pass::Outer`), which
    /// grows them by both borders' widths, where it has one, or else by its
    /// border's (`Pass::Stroke`); `None` without a GPU.
    fn koharus(layout: &crate::text::TextLayout) -> Result<Option<image::RgbaImage>> {
        let Ok(gpu) = crate::Gpu::get() else {
            eprintln!("skipped drawing without a GPU");
            return Ok(None);
        };
        let size = layout.bounds.size();
        let mut drawing = gpu.drawing(size.width.ceil() as u16, size.height.ceil() as u16);
        drawing
            .scene
            .set_transform(Affine::translate((-layout.bounds.x0, -layout.bounds.y0)));
        drawing.scene.set_paint(Color::BLACK);
        for (glyphs, run) in layout.runs.iter().zip(&layout.resolved.runs) {
            let Some((_, width)) = run.stroke else {
                continue;
            };
            let outer = run.outer_stroke.map_or(0.0, |(_, outer)| outer);
            let grow = draw::bold(&glyphs.font, run.size) + f64::from(width + outer);
            let stroke = Stroke::new(2.0 * grow).with_join(Join::Round);
            let turn = draw::turn(&glyphs.font, glyphs.sideways);
            drawing.glyphs(&glyphs.font, run.size, turn, Some(stroke), &glyphs.glyphs)?;
        }
        Ok(Some(drawing.read()?))
    }

    /// How far `ours` lies from `theirs`, by their alpha: the largest
    /// difference at a pixel; how many pixels differ by more than 8 away from
    /// the edge `theirs` antialiases, its pixels neither clear nor opaque and
    /// those beside them; and how far apart the two edges lie on average, in
    /// pixels: the differences summed, over that edge's length in pixels.
    fn difference(ours: &image::RgbaImage, theirs: &image::RgbaImage) -> (u8, usize, f64) {
        let (width, height) = theirs.dimensions();
        let edge = |x: u32, y: u32| (1..255).contains(&theirs.get_pixel(x, y).0[3]);
        let beside = |x: u32, y: u32| {
            (x.saturating_sub(1)..=(x + 1).min(width - 1))
                .any(|x| (y.saturating_sub(1)..=(y + 1).min(height - 1)).any(|y| edge(x, y)))
        };
        let (mut largest, mut off_edge, mut sum, mut length) = (0, 0, 0.0, 0);
        for (x, y, pixel) in ours.enumerate_pixels() {
            let difference = pixel.0[3].abs_diff(theirs.get_pixel(x, y).0[3]);
            largest = largest.max(difference);
            off_edge += usize::from(difference > 8 && !beside(x, y));
            sum += f64::from(difference) / 255.0;
            length += usize::from(edge(x, y));
        }
        (largest, off_edge, sum / length.max(1) as f64)
    }

    #[test]
    fn a_border_draws_as_koharus_stroke_where_it_does_not_fold() -> Result<()> {
        // Letters with no dot, which Koharu's stroke doesn't fold beside at
        // these widths: plain, under a synthetic bold and a synthetic slant,
        // and sideways in a column.
        let plain = sans();
        let mut cases = vec![("plain", plain.clone(), Direction::Horizontal)];
        let faked = [
            ("bold", fontique::FontStyle::Normal, 900.0),
            ("slanted", fontique::FontStyle::Italic, 400.0),
        ];
        for (name, style, weight) in faked {
            match crate::booharu::faked(style, weight) {
                Some(font) => cases.push((name, font, Direction::Horizontal)),
                None => eprintln!("skipped the {name} letters"),
            }
        }
        cases.push(("sideways", plain, Direction::Vertical));
        let clear = Rgba([1.0, 1.0, 1.0, 0.0]);
        let black = Rgba([0.0, 0.0, 0.0, 1.0]);
        // The border alone, under a clear fill; then the second border, under
        // a clear border, which Koharu's second pass strokes as wide as both.
        // Some widths between, as 8 px past 2 px under the synthetic bold,
        // fold where two letters' strokes meet and leave pinholes there,
        // which the region fills.
        let borders = [
            ((black, 2.0), None),
            ((black, 4.0), None),
            ((black, 8.0), None),
            ((clear, 2.0), Some((black, 4.0))),
            ((clear, 4.0), Some((black, 8.0))),
        ];
        for (name, font, direction) in &cases {
            for content in ["Hello World", "WAVE MAW"] {
                for (border, second) in borders {
                    let layout = bordered(content, font, *direction, clear, border, second)?;
                    let run = &layout.runs[0];
                    match *name {
                        "bold" => assert!(draw::bold(&run.font, 48.0) > 0.0),
                        "slanted" => assert!(draw::turn(&run.font, false).is_some()),
                        "sideways" => assert!(layout.runs.iter().all(|run| run.sideways)),
                        _ => {}
                    }
                    let Some(ours) = drawn(&layout, true)? else {
                        return Ok(());
                    };
                    let theirs = koharus(&layout)?.expect("a GPU");
                    let (largest, off_edge, apart) = difference(&ours, &theirs);
                    // vello expands Koharu's stroke, then flattens it, each
                    // within a quarter of a pixel (vello_common 0.4's `TOL`),
                    // its chords inside the curve, where the region keeps
                    // within a fiftieth. Measured on llvmpipe in DejaVu Sans:
                    // the largest difference 105 of 255, at a round join; the
                    // edges 0.074 of a pixel apart on average at most, the
                    // region's outside; and no pixel off Koharu's antialiased
                    // edge differing by more than 8, an edge which takes in
                    // the faint seam (alpha 236) its stroke leaves where
                    // World's l and d meet at 8 px. So a pixel may differ
                    // only on or beside that edge, by 144 at most, and the
                    // edges lie within 0.15 of a pixel on average, where
                    // growth off by a pixel, or mitred joins, differ by whole
                    // pixels.
                    let case = match second {
                        None => format!("{name} {content:?} at {} px", border.1),
                        Some((_, outer)) => {
                            format!("{name} {content:?} at {outer} px past {} px", border.1)
                        }
                    };
                    assert_eq!(off_edge, 0, "{case}: {off_edge} pixels off the edge");
                    assert!(largest <= 144, "{case}: {largest} of 255 apart");
                    assert!(apart <= 0.15, "{case}: {apart:.3} px apart");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn a_see_through_border_shows_one_alpha_over_its_run() -> Result<()> {
        let font = sans();
        let clear = Rgba([1.0, 1.0, 1.0, 0.0]);
        let half = Rgba([0.0, 0.0, 0.0, 0.5]);
        // Letters set close in one run under a clear fill, their glyphs grown
        // overlapping: a border at half alpha, then a second border at half
        // alpha under a clear border.
        for (border, second) in [((half, 8.0), None), ((clear, 4.0), Some((half, 4.0)))] {
            let layout = bordered(
                "WAVE MAW",
                &font,
                Direction::Horizontal,
                clear,
                border,
                second,
            )?;
            assert_eq!(layout.runs.len(), 1);
            let Some(image) = drawn(&layout, true)? else {
                return Ok(());
            };
            // Half alpha through and through, where an overlap drawn twice
            // would show three quarters, but for its antialiased edge: 7% of
            // the pixels it inks, measured in DejaVu Sans.
            let alphas: Vec<u8> = image.pixels().map(|pixel| pixel.0[3]).collect();
            let most = alphas.iter().copied().max().unwrap_or_default();
            let inked = alphas.iter().filter(|alpha| **alpha > 0).count();
            let even = alphas
                .iter()
                .filter(|alpha| alpha.abs_diff(128) <= 2)
                .count();
            assert!(most <= 130, "{most}");
            assert!(even * 5 > inked * 4, "{even} of {inked}");
        }
        Ok(())
    }
}
