//! A text layout drawn: its glyph runs filled, over their strokes, with a
//! face's synthetic bold and slant.

use anyhow::Result;
use glifo::FontEmbolden;
use kurbo::{Affine, Diagonal2, Join};
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
/// Booharu: or, by its Contour method, as the regions the layout grew from
/// its merged letters.
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
        for (glyphs, run) in layout.runs.iter().zip(&layout.resolved.runs) {
            let (paint, width, join) = match (pass, run.stroke, run.outer_stroke) {
                (Pass::Outer, Some((_, width)), Some((color, outer))) => {
                    (color, f64::from(width + outer), Join::Round)
                }
                (Pass::Stroke, Some((color, width)), _) => (color, f64::from(width), Join::Round),
                (Pass::Outer | Pass::Stroke, ..) => continue,
                (Pass::Fill, ..) => (run.fill, 0.0, Join::Miter),
            };
            let font = &glyphs.font;
            let bold = bold(font, run.size);
            drawing.scene.set_paint(Color::from(paint));
            drawing.glyphs(
                font,
                run.size,
                turn(font, glyphs.sideways),
                FontEmbolden::new(Diagonal2::new(bold + width, bold + width)).with_join(join),
                &glyphs.glyphs,
            )?;
        }
    }
    Ok(())
}

// Booharu: bold and turn, out of `draw` for contour.rs to share.
/// How far `font`'s synthetic bold grows each side of its glyphs at `size`:
/// a fiftieth of the size.
pub(super) fn bold(font: &Font, size: f32) -> f64 {
    match font.synthesis.embolden() {
        true => f64::from(size) * 0.02,
        false => 0.0,
    }
}

/// The transform `font`'s glyphs take in their own space, where y runs down:
/// glifo flips an outline before this transform (`glyph_transform * FLIP_Y`
/// in glifo 0.5's `glyph.rs`). A quarter turn clockwise on the page for
/// `sideways` text, which maps (x, y) to (-y, x), after a synthetic slant.
pub(super) fn turn(font: &Font, sideways: bool) -> Option<Affine> {
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
}
