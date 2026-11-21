//! Booharu's gradient fills, spread as 0.0.4 spread them: over the ink of
//! what they fill, never its line boxes, so a gradient runs from the edge of
//! the first letter it fills to the edge of the last however the lines and
//! sizes fall, and stays clear of an empty last line. A gradient a text's
//! style gives spreads over the whole text's ink, words with a flat colour
//! of their own included, which keep their colour; one a run of words is
//! given of their own spreads over that run's ink, across its lines. A
//! linear gradient runs at its angle, as Photoshop's does, 0 left to right
//! and 90 bottom to top, from edge to edge of the ink through its centre; a
//! radial one runs out from the ink's centre to a circle through its
//! corners. Only the fill paints a gradient: borders stay flat.

use koharu_scene::{Color, Gradient, GradientKind};
use kurbo::{Affine, Rect, Vec2};
use vello_gpu::peniko::{self, ColorStop};

use super::Unit;
use crate::text::{GlyphRun, TextRun, bold, turn};

/// `runs` with the gradient fills that `gradients`, one for each run, give
/// them, each spread over the ink of what it fills; `resolved` holds the
/// runs as the layout resolved them, and each run that spreads a gradient
/// keeps there, as its `fill_extent`, the box it spreads it over, for PSD
/// export. A text without a gradient is left as it is.
pub(crate) fn paint<'a>(
    mut runs: Vec<GlyphRun>,
    resolved: &mut [TextRun],
    gradients: impl IntoIterator<Item = Option<&'a (Gradient, Unit)>>,
) -> Vec<GlyphRun> {
    let gradients: Vec<_> = gradients.into_iter().collect();
    if gradients.iter().all(Option::is_none) {
        return runs;
    }
    let inks: Vec<Option<Rect>> = runs
        .iter()
        .zip(resolved.iter())
        .map(|(run, text)| ink(run, text.size))
        .collect();
    // What each unit spreads over: the whole text's ink, or its own runs'.
    let mut extents: Vec<(Unit, Option<Rect>)> = Vec::new();
    for ((run, text), gradient) in runs.iter_mut().zip(resolved).zip(&gradients) {
        let Some((gradient, unit)) = gradient else {
            continue;
        };
        let extent = match extents.iter().find(|(known, _)| known == unit) {
            Some((_, extent)) => *extent,
            None => {
                let extent = inks
                    .iter()
                    .zip(&gradients)
                    .filter(|(_, other)| {
                        *unit == Unit::Text || other.is_some_and(|(_, other)| other == unit)
                    })
                    .filter_map(|(ink, _)| *ink)
                    .reduce(|all, ink| all.union(ink));
                extents.push((*unit, extent));
                extent
            }
        };
        run.fill = extent.map(|extent| spread(gradient, extent));
        text.fill_extent = extent;
    }
    runs
}

/// Whether `run`'s fill hides what lies under its letters: its colour, and
/// every stop of a gradient it paints instead, opaque.
pub(crate) fn opaque(run: &TextRun) -> bool {
    run.fill.components[3] >= 1.0
        && run
            .fill_gradient
            .as_ref()
            .is_none_or(|gradient| gradient.stops.iter().all(|stop| stop.color.0[3] >= 1.0))
}

/// Where `run`'s glyphs ink at `size`, as the fill draws them: each glyph's
/// outline box grown by a faked bold, then slanted or turned and placed as
/// glifo places its outline (as `text::contour` places the letters); `None`
/// when none inks.
fn ink(run: &GlyphRun, size: f32) -> Option<Rect> {
    let bold = bold(&run.font, size);
    let turn = turn(&run.font, run.sideways).unwrap_or(Affine::IDENTITY) * Affine::FLIP_Y;
    run.glyphs
        .iter()
        .filter_map(|glyph| {
            let ink = run.font.ink(glyph.id, size)?.inflate(bold, bold);
            let at = Affine::translate((f64::from(glyph.x), f64::from(glyph.y)));
            Some((at * turn).transform_rect_bbox(ink))
        })
        .reduce(|all, ink| all.union(ink))
}

/// `gradient` spread over `extent`, in the text's own coordinates, where y
/// runs down, with its stops as given, their alpha included.
fn spread(gradient: &Gradient, extent: Rect) -> peniko::Gradient {
    let centre = extent.center();
    let paint = match gradient.kind {
        GradientKind::Linear => {
            // Photoshop's angle turns counterclockwise from the right.
            let angle = gradient.angle.to_radians();
            let along = Vec2::new(angle.cos(), -angle.sin());
            // Half the extent's length along the angle, edge to edge.
            let half = (extent.width() * along.x.abs() + extent.height() * along.y.abs()) / 2.0;
            peniko::Gradient::new_linear(centre - along * half, centre + along * half)
        }
        GradientKind::Radial => {
            let radius = extent.width().hypot(extent.height()) / 2.0;
            peniko::Gradient::new_radial(centre, radius as f32)
        }
    };
    let stops: Vec<ColorStop> = gradient
        .stops
        .iter()
        .map(|stop| ColorStop::from((stop.offset as f32, Color::from(stop.color))))
        .collect();
    paint.with_stops(stops.as_slice())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use fontique::FontStyle;
    use koharu_scene::{CharStyle, Direction, GradientStop, Rgba, RichText, Span, Text, TextStyle};
    use kurbo::Point;

    use super::*;
    use crate::{
        booharu::{faked, static_sans},
        fonts::Font,
        text::{TextLayout, layout},
    };

    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
    const BLUE: Rgba = Rgba([0.0, 0.0, 1.0, 1.0]);
    const GREEN: Rgba = Rgba([0.0, 0.5, 0.0, 1.0]);

    /// A gradient of `kind` at `angle` from red to blue.
    fn gradient(kind: GradientKind, angle: f64) -> Gradient {
        Gradient {
            kind,
            angle,
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: RED,
                },
                GradientStop {
                    offset: 1.0,
                    color: BLUE,
                },
            ],
        }
    }

    /// A style with `gradient`.
    fn filled(gradient: Gradient) -> CharStyle {
        CharStyle {
            fill_gradient: Some(gradient),
            ..CharStyle::default()
        }
    }

    /// A style of `size`, over `style`.
    fn sized(size: f64, style: CharStyle) -> CharStyle {
        CharStyle {
            size: Some(size),
            ..style
        }
    }

    /// `spans`, each words and their own style, laid out in `font` as
    /// point text styled `style`.
    fn laid(font: &Font, style: CharStyle, spans: Vec<(&str, CharStyle)>) -> Result<TextLayout> {
        let text = Text {
            content: RichText(
                spans
                    .into_iter()
                    .map(|(words, attributes)| Span {
                        insert: words.to_owned(),
                        attributes,
                    })
                    .collect(),
            ),
            style: TextStyle {
                chars: style,
                ..TextStyle::default()
            },
            ..Text::default()
        };
        Ok(layout(&text, |_, _| Ok(vec![font.clone()]))?.expect("a layout"))
    }

    /// The ink of the glyphs of `layout`'s runs that `keep` keeps, by their
    /// resolved runs: each glyph's outline box, upright, set on its origin.
    fn ink_of(layout: &TextLayout, keep: impl Fn(&TextRun) -> bool) -> Rect {
        layout
            .runs
            .iter()
            .zip(&layout.resolved.runs)
            .filter(|(_, resolved)| keep(resolved))
            .flat_map(|(run, resolved)| {
                run.glyphs.iter().filter_map(move |glyph| {
                    let ink = run.font.ink(glyph.id, resolved.size)?;
                    let at = Vec2::new(f64::from(glyph.x), f64::from(glyph.y));
                    Some(Rect::new(ink.x0, -ink.y1, ink.x1, -ink.y0) + at)
                })
            })
            .reduce(|all, ink| all.union(ink))
            .expect("the letters ink")
    }

    /// The box around the ink of `layout`'s first run, at 24 pixels: each
    /// glyph's outline box grown by `grow`, its corners, each across and up
    /// from the glyph's origin, placed by `corner`.
    fn placed(layout: &TextLayout, grow: f64, corner: impl Fn(Point, f64, f64) -> Point) -> Rect {
        let run = &layout.runs[0];
        run.glyphs
            .iter()
            .filter_map(|glyph| {
                let ink = run.font.ink(glyph.id, 24.0)?.inflate(grow, grow);
                let at = Point::new(f64::from(glyph.x), f64::from(glyph.y));
                let corners = [
                    (ink.x0, ink.y0),
                    (ink.x0, ink.y1),
                    (ink.x1, ink.y0),
                    (ink.x1, ink.y1),
                ];
                Some(corners.map(|(across, up)| corner(at, across, up)))
            })
            .flatten()
            .map(|point| Rect::from_points(point, point))
            .reduce(|all, point| all.union(point))
            .expect("the letters ink")
    }

    /// Where a linear gradient starts and ends.
    fn ends(gradient: &peniko::Gradient) -> (Point, Point) {
        match gradient.kind {
            peniko::GradientKind::Linear(line) => (line.start, line.end),
            _ => panic!("a linear gradient: {gradient:?}"),
        }
    }

    fn near(a: Point, b: Point) -> bool {
        (a - b).hypot() < 1e-6
    }

    /// Whether a run's linear gradient runs left to right across `ink`,
    /// through its middle.
    fn across(fill: Option<&peniko::Gradient>, ink: Rect) -> bool {
        let (start, end) = ends(fill.expect("a gradient fill"));
        let middle = ink.center().y;
        near(start, Point::new(ink.x0, middle)) && near(end, Point::new(ink.x1, middle))
    }

    /// Whether `extent` is `ink`, to rounding.
    fn kept(extent: Option<Rect>, ink: Rect) -> bool {
        extent.is_some_and(|extent| {
            near(extent.origin(), ink.origin())
                && near(Point::new(extent.x1, extent.y1), Point::new(ink.x1, ink.y1))
        })
    }

    #[test]
    fn a_gradient_spreads_along_its_angle_or_out_to_the_corners() {
        let extent = Rect::new(10.0, 20.0, 110.0, 70.0);
        // 0 runs left to right through the middle, and 90 bottom to top.
        let (start, end) = ends(&spread(&gradient(GradientKind::Linear, 0.0), extent));
        assert!(near(start, Point::new(10.0, 45.0)), "{start:?}");
        assert!(near(end, Point::new(110.0, 45.0)), "{end:?}");
        let (start, end) = ends(&spread(&gradient(GradientKind::Linear, 90.0), extent));
        assert!(near(start, Point::new(60.0, 70.0)), "{start:?}");
        assert!(near(end, Point::new(60.0, 20.0)), "{end:?}");
        // 45 runs up to the right, its ends level with the far corners.
        let (start, end) = ends(&spread(&gradient(GradientKind::Linear, 45.0), extent));
        let along = (end - start).normalize();
        assert!(((Point::new(10.0, 70.0) - start).dot(along)).abs() < 1e-9);
        assert!(((Point::new(110.0, 20.0) - end).dot(along)).abs() < 1e-9);
        // A radial one runs from the centre to the corners, whatever its
        // angle, with its stops as given, their alpha included.
        let mut radial = gradient(GradientKind::Radial, 30.0);
        radial.stops[1].color.0[3] = 0.5;
        let radial = spread(&radial, extent);
        let peniko::GradientKind::Radial(circle) = radial.kind else {
            panic!("a radial gradient: {radial:?}");
        };
        assert_eq!(
            (circle.start_center, circle.end_center),
            (extent.center(), extent.center())
        );
        assert!((f64::from(circle.end_radius) - 125.0_f64.sqrt() * 5.0).abs() < 1e-4);
        let stops: Vec<(f32, [f32; 4])> = radial
            .stops
            .iter()
            .map(|stop| (stop.offset, stop.color.components))
            .collect();
        assert_eq!(stops, [(0.0, RED.0), (1.0, [0.0, 0.0, 1.0, 0.5])]);
    }

    #[test]
    fn a_gradient_spreads_over_the_ink_as_it_draws_bold_slanted_or_turned() -> Result<()> {
        let (Some(plain), Some(faked)) = (static_sans(), faked(FontStyle::Italic, 900.0)) else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        let style = filled(gradient(GradientKind::Linear, 0.0));
        // A faked bold grows each glyph a fiftieth of its size each side,
        // and a faked slant leans its top right by its height times the
        // slant's tangent.
        let bold = laid(&faked, style.clone(), vec![("Hil", CharStyle::default())])?;
        assert!(faked.synthesis.embolden());
        let tan = f64::from(14.0_f32.to_radians().tan());
        let ink = placed(&bold, 24.0 * 0.02, |at, across, up| {
            at + Vec2::new(across + up * tan, -up)
        });
        let fill = bold.runs[0].fill.as_ref();
        assert!(across(fill, ink), "{fill:?} over {ink:?}");
        // A column turns Latin a quarter clockwise, its tops to the right,
        // so the gradient runs across the column.
        let column = Text {
            content: "Lettering".into(),
            style: TextStyle {
                chars: style,
                ..TextStyle::default()
            },
            direction: Direction::Vertical,
            ..Text::default()
        };
        let turned = layout(&column, |_, _| Ok(vec![plain.clone()]))?.expect("a layout");
        assert_eq!(turned.runs.len(), 1);
        assert!(turned.runs[0].sideways);
        let ink = placed(&turned, 0.0, |at, across, up| at + Vec2::new(up, across));
        assert!(ink.height() > ink.width(), "{ink:?}");
        let fill = turned.runs[0].fill.as_ref();
        assert!(across(fill, ink), "{fill:?} over {ink:?}");
        Ok(())
    }

    #[test]
    fn a_text_s_gradient_spreads_over_its_ink_clear_of_an_empty_last_line() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        // Two lines of two sizes, and the empty line a last line break ends
        // with, under a gradient running bottom to top.
        let layout = laid(
            &font,
            filled(gradient(GradientKind::Linear, 90.0)),
            vec![
                ("Big\n", sized(48.0, CharStyle::default())),
                ("small\n", sized(16.0, CharStyle::default())),
            ],
        )?;
        assert_eq!(layout.lines.len(), 3);
        let ink = ink_of(&layout, |_| true);
        assert!(ink.y1 < f64::from(layout.lines[2].top), "{ink:?}");
        assert!(layout.bounds.y1 > f64::from(layout.lines[2].top));
        // It runs from the bottom of the ink to its top, through its middle.
        let (bottom, top) = (
            Point::new(ink.center().x, ink.y1),
            Point::new(ink.center().x, ink.y0),
        );
        assert_eq!(layout.runs.len(), 2);
        for run in &layout.runs {
            let (start, end) = ends(run.fill.as_ref().expect("a gradient fill"));
            assert!(near(start, bottom) && near(end, top), "{start:?} {end:?}");
        }
        Ok(())
    }

    #[test]
    fn a_word_s_own_gradient_spreads_over_it_across_its_lines_and_sizes() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        // Two spans of two sizes given one gradient, broken across two lines
        // between plain words, the second line's reaching further.
        let own = filled(gradient(GradientKind::Linear, 0.0));
        let layout = laid(
            &font,
            CharStyle::default(),
            vec![
                ("plain ", CharStyle::default()),
                ("Big\n", sized(40.0, own.clone())),
                ("small", sized(20.0, own)),
                (" and a plain tail", CharStyle::default()),
            ],
        )?;
        assert_eq!(layout.lines.len(), 2);
        let words = ink_of(&layout, |run| run.fill_gradient.is_some());
        let whole = ink_of(&layout, |_| true);
        assert!(words.x1 + 10.0 < whole.x1, "{words:?} in {whole:?}");
        let second = f64::from(layout.lines[1].top);
        assert!(words.y0 < second && words.y1 > second, "{words:?}");
        let mut painted = 0;
        for (run, resolved) in layout.runs.iter().zip(&layout.resolved.runs) {
            match resolved.fill_gradient {
                Some(_) => {
                    painted += 1;
                    assert!(across(run.fill.as_ref(), words), "{:?}", run.fill);
                }
                None => assert!(run.fill.is_none()),
            }
        }
        assert_eq!(painted, 2);
        Ok(())
    }

    #[test]
    fn words_apart_given_one_gradient_spread_it_each_over_their_own() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        let own = filled(gradient(GradientKind::Linear, 0.0));
        let layout = laid(
            &font,
            CharStyle::default(),
            vec![
                ("one", own.clone()),
                (" and ", CharStyle::default()),
                ("two", own),
            ],
        )?;
        let [one, _, two] = &layout.runs[..] else {
            panic!("three runs");
        };
        let ink = |at: usize| {
            let keep = layout.resolved.runs[at].range.clone();
            ink_of(&layout, |run| run.range == keep)
        };
        assert!(across(one.fill.as_ref(), ink(0)));
        assert!(across(two.fill.as_ref(), ink(2)));
        Ok(())
    }

    #[test]
    fn a_word_of_a_flat_colour_in_a_gradient_text_keeps_its_colour() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        // A bigger word given a colour, which ends the text.
        let flat = CharStyle {
            fill: Some(GREEN),
            ..CharStyle::default()
        };
        let layout = laid(
            &font,
            filled(gradient(GradientKind::Linear, 0.0)),
            vec![
                ("Gradient ", CharStyle::default()),
                ("Flat", sized(40.0, flat)),
            ],
        )?;
        let ([words, flat], [painted, green]) = (&layout.runs[..], &layout.resolved.runs[..])
        else {
            panic!("two runs");
        };
        assert_eq!(
            (&flat.fill, green.fill.components, &green.fill_gradient),
            (&None, GREEN.0, &None)
        );
        assert!(painted.fill_gradient.is_some());
        // The rest spreads the gradient over the whole text's ink, the flat
        // word's too.
        let whole = ink_of(&layout, |_| true);
        assert!(across(words.fill.as_ref(), whole), "{:?}", words.fill);
        Ok(())
    }

    #[test]
    fn each_run_keeps_the_box_its_gradient_spreads_over() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        // A text's gradient spreads over the whole text's ink, so each run
        // it fills keeps that box, whatever its size.
        let text = laid(
            &font,
            filled(gradient(GradientKind::Linear, 30.0)),
            vec![
                ("Big ", sized(40.0, CharStyle::default())),
                ("small", CharStyle::default()),
            ],
        )?;
        let whole = ink_of(&text, |_| true);
        assert_eq!(text.resolved.runs.len(), 2);
        for run in &text.resolved.runs {
            assert!(
                kept(run.fill_extent, whole),
                "{:?} in {whole:?}",
                run.fill_extent
            );
        }
        // Words given a gradient of their own keep their own ink, and the
        // words around them under the text's gradient the whole text's.
        let layout = laid(
            &font,
            filled(gradient(GradientKind::Linear, 0.0)),
            vec![
                ("Text ", CharStyle::default()),
                ("own", filled(gradient(GradientKind::Radial, 0.0))),
                (" text", CharStyle::default()),
            ],
        )?;
        let [before, own, after] = &layout.resolved.runs[..] else {
            panic!("three runs");
        };
        let whole = ink_of(&layout, |_| true);
        let words = ink_of(&layout, |run| run.range == own.range);
        assert!(
            words.width() + 10.0 < whole.width(),
            "{words:?} in {whole:?}"
        );
        assert!(kept(own.fill_extent, words), "{:?}", own.fill_extent);
        assert!(kept(before.fill_extent, whole), "{:?}", before.fill_extent);
        assert!(kept(after.fill_extent, whole), "{:?}", after.fill_extent);
        // Without a gradient, a run keeps none.
        let flat = CharStyle {
            fill: Some(GREEN),
            ..CharStyle::default()
        };
        let plain = laid(&font, flat, vec![("Plain", CharStyle::default())])?;
        assert_eq!(plain.resolved.runs[0].fill_extent, None);
        Ok(())
    }

    #[test]
    fn a_gradient_run_s_flat_fill_is_its_first_stop() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        // A gradient wins over a fill beside it.
        let style = CharStyle {
            fill: Some(GREEN),
            ..filled(gradient(GradientKind::Radial, 0.0))
        };
        let layout = laid(&font, style, vec![("Words", CharStyle::default())])?;
        let run = &layout.resolved.runs[0];
        assert_eq!(run.fill.components, RED.0);
        assert_eq!(run.fill_gradient, Some(gradient(GradientKind::Radial, 0.0)));
        assert!(matches!(
            layout.runs[0].fill.as_ref().map(|fill| fill.kind),
            Some(peniko::GradientKind::Radial(_))
        ));
        Ok(())
    }

    #[test]
    fn a_border_shows_through_a_gradient_only_where_a_stop_is_translucent() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        // A bordered I, whose stem's middle its Contour border covers under
        // an opaque fill, and leaves clear under a translucent one.
        let laid = |alpha: f32| -> Result<TextLayout> {
            let mut gradient = gradient(GradientKind::Linear, 0.0);
            gradient.stops[1].color.0[3] = alpha;
            let style = CharStyle {
                size: Some(100.0),
                stroke: Some(Rgba([1.0, 1.0, 1.0, 1.0])),
                stroke_width: Some(4.0),
                ..filled(gradient)
            };
            laid(&font, style, vec![("I", CharStyle::default())])
        };
        let border = |layout: &TextLayout| {
            let regions = layout.contours.as_ref().expect("Contour is the default");
            assert_eq!(regions.len(), 1);
            regions[0].1.clone()
        };
        let (opaque, clear) = (laid(1.0)?, laid(0.5)?);
        assert!(super::opaque(&opaque.resolved.runs[0]));
        assert!(!super::opaque(&clear.resolved.runs[0]));
        let stem = ink_of(&opaque, |_| true).center();
        assert!(kurbo::Shape::contains(&border(&opaque), stem));
        assert!(!kurbo::Shape::contains(&border(&clear), stem));
        Ok(())
    }
}
