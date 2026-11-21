//! A text layout drawn: its glyph runs filled, over their strokes, with a
//! face's synthetic bold and slant.

use anyhow::Result;
use kurbo::{Affine, Join, Stroke};
use vello_gpu::peniko::{Color, Fill}; // Booharu: Fill, for the Contour borders.

use super::TextLayout;
use crate::{Drawing, fonts::Font}; // Booharu: Font, for bold and turn.

/// What one pass over a layout's glyph runs draws.
#[derive(Clone, Copy)]
enum Pass {
    /// Booharu: a second border, the glyphs grown past their stroke.
    Outer,
    Stroke,
    Fill,
}

/// Draws `layout`'s glyph runs onto `drawing` through `transform`, its
/// strokes too when `strokes` says so. Strokes draw first, under every fill,
/// as Photoshop draws an Outside stroke under its type: as the glyphs grown
/// by the stroke's width with round joins.
/// Booharu: each run's border filled as the region its glyphs grown cover,
/// which the layout grew once (`TextLayout::grown`), in place of a stroke,
/// which folds beside a dot and leaves holes there; or, by its Contour
/// method, as the regions the layout grew from its merged letters.
pub(crate) fn draw(
    drawing: &mut Drawing,
    layout: &TextLayout,
    transform: Affine,
    strokes: bool,
) -> Result<()> {
    drawing.scene.set_transform(transform);
    // Booharu: Contour borders fill their regions under every fill; otherwise a
    // second border draws under the stroke, and the stroke under the fill.
    let passes: &[Pass] = match (strokes, &layout.contours) {
        (true, Some(regions)) => {
            drawing.scene.set_fill_rule(Fill::NonZero);
            for (color, region) in regions {
                drawing.scene.set_paint(*color);
                drawing.scene.fill_path(region);
            }
            &[Pass::Fill]
        }
        (true, None) => &[Pass::Outer, Pass::Stroke, Pass::Fill],
        (false, _) => &[Pass::Fill],
    };
    for &pass in passes {
        // Booharu: `at`, the run's place, for its grown borders.
        for (at, (glyphs, run)) in layout.runs.iter().zip(&layout.resolved.runs).enumerate() {
            let (paint, width, join) = match (pass, run.stroke, run.outer_stroke) {
                (Pass::Outer, Some((_, width)), Some((color, outer))) => {
                    (color, f64::from(width + outer), Join::Round)
                }
                (Pass::Stroke, Some((color, width)), _) => (color, f64::from(width), Join::Round),
                (Pass::Outer | Pass::Stroke, ..) => continue,
                (Pass::Fill, ..) => (run.fill, 0.0, Join::Miter),
            };
            let font = &glyphs.font;
            let bold = bold(font, run.size); // Booharu: shared, as contour.rs grows by it too.
            let turn = turn(font, glyphs.sideways); // Booharu: shared, as contour.rs turns by it too.
            // Glyphs grow under a stroke twice as wide, as Skia fakes a bold,
            // not by glifo's embolden: kurbo 0.13.1's `expand_path` fits one
            // cubic to each segment's offset, so a short handle turning back,
            // as many comic fonts have, sends the outline far off the glyph,
            // and it grows every contour one way, by the glyph's area, so a
            // contour wound against the rest, like some fonts' dots on i and
            // j, shrinks instead (koharu-rs/koharu#1073).
            // Booharu: but where the stroke's half-width passes a small
            // contour, as beside a dot once a border is wider than about a
            // fifth of the size, its inner side folds back past the contour
            // and the folds cancel by the non-zero rule, leaving holes, so
            // the border passes below fill the run's glyphs grown instead
            // (contour.rs); the fill's synthetic bold still strokes here.
            let grow = bold + width;
            let stroke = (grow > 0.0).then(|| Stroke::new(2.0 * grow).with_join(join));
            // Booharu: a border pass fills the run's glyphs grown by `grow`
            // with round joins, as the layout grew them: what the fill and
            // the stroke cover together where the stroke doesn't fold, with
            // no holes, and one shape, so a translucent paint needs no layer.
            let grown = layout.grown.get(at).and_then(|grown| match pass {
                Pass::Outer => grown.outer.as_ref(),
                Pass::Stroke => grown.stroke.as_ref(),
                Pass::Fill => None,
            });
            if let Some(region) = grown.filter(|_| stroke.is_some()) {
                super::contour::fill(drawing, region, paint);
                continue;
            }
            // Booharu: a gradient fill paints in place of the flat one.
            let gradient = match pass {
                Pass::Fill => glyphs.fill.as_ref(),
                Pass::Outer | Pass::Stroke => None,
            };
            // The stroke overlaps the fill inside the outline, so a
            // translucent paint draws opaque in a layer of its alpha, or the
            // overlap would show darker.
            let color = Color::from(paint);
            let alpha = color.components[3];
            let layered = stroke.is_some() && alpha < 1.0 && gradient.is_none(); // Booharu: a gradient stays one paint.
            if layered {
                drawing.scene.push_opacity_layer(alpha);
            }
            drawing.scene.set_paint(match layered {
                true => color.with_alpha(1.0),
                false => color,
            });
            // Booharu: the gradient, over the flat paint set above.
            if let Some(gradient) = gradient {
                drawing.scene.set_paint(gradient.clone());
            }
            drawing.glyphs(font, run.size, turn, stroke, &glyphs.glyphs)?;
            if layered {
                drawing.scene.pop_layer();
            }
        }
    }
    Ok(())
}

// Booharu: bold and turn, out of `draw` for contour.rs to share, bold for the
// bounds' room for faked bold and slant (booharu/lines.rs), and both for the
// ink a gradient fill spreads over (booharu/paint.rs).
/// How far `font`'s synthetic bold grows each side of its glyphs at `size`:
/// a fiftieth of the size.
pub(crate) fn bold(font: &Font, size: f32) -> f64 {
    match font.synthesis.embolden() {
        true => f64::from(size) * 0.02,
        false => 0.0,
    }
}

/// The transform `font`'s glyphs take in their own space, where y runs down:
/// glifo flips an outline before this transform (`glyph_transform * FLIP_Y`
/// in glifo 0.5's `glyph.rs`). A quarter turn clockwise on the page for
/// `sideways` text, which maps (x, y) to (-y, x), after a synthetic slant.
pub(crate) fn turn(font: &Font, sideways: bool) -> Option<Affine> {
    let skew = font
        .synthesis
        .skew()
        .map(|angle| Affine::skew(-f64::from(angle.to_radians().tan()), 0.0));
    match sideways {
        true => {
            Some(Affine::new([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]) * skew.unwrap_or(Affine::IDENTITY))
        }
        false => skew,
    }
}

#[cfg(test)]
mod tests {
    use image::RgbaImage;
    use koharu_scene::{CharStyle, Rgba, Text, TextStyle};

    use super::*;
    use crate::{Gpu, fonts::Library};

    #[test]
    fn strokes_widen_a_text_and_draw_under_its_fill() -> Result<()> {
        let mut library = Library::new();
        let font = ["Arial", "DejaVu Sans", "Helvetica"]
            .into_iter()
            .find_map(|family| library.family(family).ok())
            .expect("an installed sans-serif");
        let laid = |stroke_width| -> Result<TextLayout> {
            let text = Text {
                content: "Stroked".into(),
                style: TextStyle {
                    chars: CharStyle {
                        stroke: Some(Rgba([1.0, 0.0, 0.0, 1.0])),
                        stroke_width,
                        ..CharStyle::default()
                    },
                    ..TextStyle::default()
                },
                ..Text::default()
            };
            Ok(crate::text::layout(&text, |_, _| Ok(vec![font.clone()]))?.expect("a layout"))
        };
        let (plain, stroked) = (laid(None)?, laid(Some(3.0))?);
        assert_eq!(stroked.bounds, plain.bounds.inflate(3.0, 3.0));
        let Ok(gpu) = Gpu::get() else {
            eprintln!("skipped drawing without a GPU");
            return Ok(());
        };
        // Drawn on white, the stroked text is red around a black fill.
        let draw = |layout: &TextLayout| -> Result<RgbaImage> {
            let size = layout.bounds.size();
            let mut drawing = gpu.drawing(size.width.ceil() as u16, size.height.ceil() as u16);
            draw(
                &mut drawing,
                layout,
                Affine::translate((-layout.bounds.x0, -layout.bounds.y0)),
                true,
            )?;
            drawing.read()
        };
        let red = |image: &RgbaImage| {
            image
                .pixels()
                .filter(|pixel| pixel.0[0] > 200 && pixel.0[1] < 60 && pixel.0[3] > 200)
                .count()
        };
        let (plain, stroked) = (draw(&plain)?, draw(&stroked)?);
        assert_eq!(red(&plain), 0);
        assert!(red(&stroked) > 100, "{} red pixels", red(&stroked));
        assert!(
            stroked
                .pixels()
                .any(|pixel| pixel.0[..3] == [0, 0, 0] && pixel.0[3] == 255)
        );
        Ok(())
    }

    #[test]
    fn a_sideways_glyph_turns_a_quarter_clockwise_inside_its_bounds() -> Result<()> {
        let mut library = Library::new();
        let font = ["Arial", "DejaVu Sans", "Helvetica"]
            .into_iter()
            .find_map(|family| library.family(family).ok())
            .expect("an installed sans-serif");
        let text = Text {
            content: "L".into(),
            direction: koharu_scene::Direction::Vertical,
            ..Text::default()
        };
        let layout = crate::text::layout(&text, |_, _| Ok(vec![font.clone()]))?.expect("a layout");
        assert!(layout.runs[0].sideways);
        let Ok(gpu) = Gpu::get() else {
            eprintln!("skipped drawing without a GPU");
            return Ok(());
        };
        // Drawn with a margin round its bounds, so ink a wrong turn moves out
        // of them still shows.
        let margin = 32.0;
        let size = layout.bounds.size();
        let mut drawing = gpu.drawing(
            (size.width + margin * 2.0).ceil() as u16,
            (size.height + margin * 2.0).ceil() as u16,
        );
        let origin = (margin - layout.bounds.x0, margin - layout.bounds.y0);
        draw(&mut drawing, &layout, Affine::translate(origin), false)?;
        let image = drawing.read()?;
        let ink: Vec<(u32, u32)> = image
            .enumerate_pixels()
            .filter(|(_, _, pixel)| pixel.0[3] > 128 && pixel.0[0] < 128)
            .map(|(x, y, _)| (x, y))
            .collect();
        let left = ink.iter().map(|at| at.0).min().expect("ink");
        let right = ink.iter().map(|at| at.0).max().expect("ink");
        let top = ink.iter().map(|at| at.1).min().expect("ink");
        let bottom = ink.iter().map(|at| at.1).max().expect("ink");
        let inside = |low: u32, high: u32, from: f64, length: f64| {
            f64::from(low) >= from - 1.0 && f64::from(high) <= from + length + 1.0
        };
        assert!(
            inside(left, right, margin, size.width),
            "{left}..{right} across"
        );
        assert!(
            inside(top, bottom, margin, size.height),
            "{top}..{bottom} down"
        );
        // An L turned clockwise is a Γ: its stem runs along the top, which
        // turned the other way would be a ┘'s foot at the right.
        let along_top = ink.iter().filter(|at| at.1 <= top + 1).count() as u32;
        assert!(
            along_top * 2 > right - left,
            "{along_top} of {}",
            right - left
        );
        Ok(())
    }

    // Booharu: a gradient fill paints in place of the flat one, across the
    // text where the drawing puts it, while its border stays flat.
    #[test]
    fn a_gradient_fill_runs_across_the_text_where_it_is_drawn() -> Result<()> {
        let mut library = Library::new();
        let font = ["Arial", "DejaVu Sans", "Helvetica"]
            .into_iter()
            .find_map(|family| library.family(family).ok())
            .expect("an installed sans-serif");
        let stop = |offset, color| koharu_scene::GradientStop { offset, color };
        let text = Text {
            content: "IIIIII".into(),
            style: TextStyle {
                chars: CharStyle {
                    size: Some(48.0),
                    stroke: Some(Rgba([0.0, 1.0, 0.0, 1.0])),
                    stroke_width: Some(3.0),
                    fill_gradient: Some(koharu_scene::Gradient {
                        kind: koharu_scene::GradientKind::Linear,
                        angle: 0.0,
                        stops: vec![
                            stop(0.0, Rgba([1.0, 0.0, 0.0, 1.0])),
                            stop(1.0, Rgba([0.0, 0.0, 1.0, 1.0])),
                        ],
                    }),
                    ..CharStyle::default()
                },
                // No border method: Koharu's border, drawn in a pass before
                // the fill's.
                ..TextStyle::default()
            },
            ..Text::default()
        };
        let layout = crate::text::layout(&text, |_, _| Ok(vec![font.clone()]))?.expect("a layout");
        let Ok(gpu) = Gpu::get() else {
            eprintln!("skipped drawing without a GPU");
            return Ok(());
        };
        // Drawn well in from the drawing's left edge.
        let margin = 100.0;
        let size = layout.bounds.size();
        let mut drawing = gpu.drawing(
            (size.width + margin).ceil() as u16,
            size.height.ceil() as u16,
        );
        let origin = (margin - layout.bounds.x0, -layout.bounds.y0);
        draw(&mut drawing, &layout, Affine::translate(origin), true)?;
        let image = drawing.read()?;
        // The letters' pixels, which show no green, red at the left end and
        // blue at the right.
        let ink: Vec<(u32, [u8; 4])> = image
            .enumerate_pixels()
            .filter(|(_, _, pixel)| pixel.0[1] < 64 && pixel.0[3] > 200)
            .map(|(x, _, pixel)| (x, pixel.0))
            .collect();
        let left = ink.iter().map(|at| at.0).min().expect("ink");
        let right = ink.iter().map(|at| at.0).max().expect("ink");
        assert!(f64::from(left) >= margin - 1.0, "{left}");
        let mut starts = ink.iter().filter(|(x, _)| *x < left + 4);
        assert!(starts.all(|(_, pixel)| pixel[0] > pixel[2] + 128));
        let mut ends = ink.iter().filter(|(x, _)| *x + 4 > right);
        assert!(ends.all(|(_, pixel)| pixel[2] > pixel[0] + 128));
        // The border around them stays green.
        let green = |pixel: &&image::Rgba<u8>| pixel.0[1] > 200 && pixel.0[0].max(pixel.0[2]) < 60;
        assert!(image.pixels().filter(green).count() > 100);
        Ok(())
    }
}
