//! Where the runs of a horizontal line stand, and how far a face's faked
//! bold and slant carry its ink. Koharu centres each run of a line in the
//! line's pitch by its own font's ascent and descent, so in a line of two
//! sizes, or of a Latin font and a CJK fallback, each stands on a baseline
//! of its own and the smaller words float up the line. Booharu stands a
//! line's runs on one baseline, as Photoshop and hand lettering do (D23):
//! the line's largest ascent and largest descent, centred together in its
//! pitch as Koharu centres a run alone, so a line of one size and font
//! stands where it did. Vertical text centres each run across its column
//! as before. The bold and slant fontique fakes on a face (D12) carry ink
//! past the glyphs' boxes, which a layout's bounds make room for, so
//! nothing drawn inside them is cut.

use crate::{fonts::Font, text::bold};

/// The baseline of a horizontal line `pitch` tall from `top`, whose runs
/// reach `extents` above and below it, each an ascent and a descent: its
/// largest ascent and largest descent, centred together in the pitch.
pub(crate) fn shared_baseline(
    top: f32,
    pitch: f32,
    extents: impl IntoIterator<Item = (f32, f32)>,
) -> f32 {
    let (ascent, descent) = extents
        .into_iter()
        .fold((0.0_f32, 0.0_f32), |(most_up, most_down), (up, down)| {
            (most_up.max(up), most_down.max(down))
        });
    top + (pitch - (ascent + descent)) * 0.5 + ascent
}

/// How far the faked bold and slant of the furthest of `runs`, each a font
/// at a size, carry its ink past its glyphs' boxes, whichever way: the
/// bold's growth on each side (`text::bold`), and the slant's at the
/// furthest the face reaches from its baseline, its ascent or its descent.
pub(crate) fn faux_reach<'a>(runs: impl IntoIterator<Item = (&'a Font, f32)>) -> f32 {
    runs.into_iter()
        .map(|(font, size)| {
            let (ascent, descent) = font.extent(size);
            let slant = font.synthesis.skew().map_or(0.0, |angle| {
                angle.to_radians().tan().abs() * ascent.max(descent)
            });
            bold(font, size) as f32 + slant
        })
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use fontique::FontStyle;
    use koharu_scene::{CharStyle, Direction, RichText, Span, Text};
    use kurbo::Point;

    use super::*;
    use crate::{
        booharu::{faked, static_sans},
        text::{TextLayout, layout},
    };

    /// `spans`, each words at a size, laid out as point text in `font`.
    fn laid(font: &Font, spans: &[(&str, f64)], direction: Direction) -> Result<TextLayout> {
        let text = Text {
            content: RichText(
                spans
                    .iter()
                    .map(|&(words, size)| Span {
                        insert: words.to_owned(),
                        attributes: CharStyle {
                            size: Some(size),
                            ..CharStyle::default()
                        },
                    })
                    .collect(),
            ),
            direction,
            ..Text::default()
        };
        Ok(layout(&text, |_, _| Ok(vec![font.clone()]))?.expect("a layout"))
    }

    #[test]
    fn a_line_s_baseline_makes_room_for_its_largest_ascent_and_descent() {
        assert_eq!(
            shared_baseline(10.0, 40.0, [(20.0, 5.0), (10.0, 8.0)]),
            36.0
        );
        // One run alone stands where Koharu centres it.
        let (top, pitch, ascent, descent) = (3.0_f32, 28.8_f32, 21.5_f32, 5.25_f32);
        assert_eq!(
            shared_baseline(top, pitch, [(ascent, descent)]),
            top + (pitch - (ascent + descent)) * 0.5 + ascent
        );
    }

    #[test]
    fn a_line_of_two_sizes_stands_on_one_baseline() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        let layout = laid(
            &font,
            &[("Big ", 32.0), ("small", 16.0)],
            Direction::Horizontal,
        )?;
        assert_eq!(layout.lines.len(), 1);
        let ys: Vec<f32> = layout
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.y))
            .collect();
        assert!(ys.windows(2).all(|pair| pair[0] == pair[1]), "{ys:?}");
        // Under the larger size's ascent, centred with its descent.
        let line = &layout.lines[0];
        assert_eq!(
            ys[0],
            shared_baseline(line.top, line.pitch, [font.extent(32.0)])
        );
        assert_eq!(layout.resolved.baseline, ys[0]);
        Ok(())
    }

    #[test]
    fn a_line_of_one_size_stands_where_it_did() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        let layout = laid(&font, &[("Plain words", 24.0)], Direction::Horizontal)?;
        let line = &layout.lines[0];
        let (ascent, descent) = font.extent(24.0);
        let koharu = line.top + (line.pitch - (ascent + descent)) * 0.5 + ascent;
        assert!(
            layout.runs[0].glyphs.iter().all(|glyph| glyph.y == koharu),
            "{koharu}"
        );
        Ok(())
    }

    #[test]
    fn a_column_centres_each_sideways_run_by_its_own_size() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        let layout = laid(
            &font,
            &[("Big", 32.0), ("small", 16.0)],
            Direction::Vertical,
        )?;
        let line = &layout.lines[0];
        let center = line.top + line.pitch * 0.5;
        assert_eq!(layout.runs.len(), 2);
        for (run, text) in layout.runs.iter().zip(&layout.resolved.runs) {
            assert!(run.sideways);
            let (ascent, descent) = font.extent(text.size);
            let x = -center - (ascent - descent) * 0.5;
            assert!(run.glyphs.iter().all(|glyph| glyph.x == x), "{x}");
        }
        Ok(())
    }

    #[test]
    fn a_run_says_which_of_its_bold_and_slant_are_faked() -> Result<()> {
        let (Some(plain), Some(both), Some(slanted)) = (
            static_sans(),
            faked(FontStyle::Italic, 700.0),
            faked(FontStyle::Italic, 400.0),
        ) else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        let flags = |font: &Font| -> Result<(bool, bool)> {
            let layout = laid(font, &[("Words", 24.0)], Direction::Horizontal)?;
            let run = &layout.resolved.runs[0];
            Ok((run.faux_bold, run.faux_italic))
        };
        assert_eq!(flags(&both)?, (true, true));
        assert_eq!(flags(&slanted)?, (false, true));
        assert_eq!(flags(&plain)?, (false, false));
        Ok(())
    }

    #[test]
    fn a_slanted_text_s_bounds_hold_its_slanted_ink() -> Result<()> {
        let (Some(plain), Some(slanted)) = (static_sans(), faked(FontStyle::Italic, 400.0)) else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        // Tall letters, the last leaning past the line's end.
        let words = [("Hil", 24.0)];
        let (upright, leaning) = (
            laid(&plain, &words, Direction::Horizontal)?,
            laid(&slanted, &words, Direction::Horizontal)?,
        );
        let tan = f64::from(14.0_f32.to_radians().tan());
        // Each corner of each glyph's ink, slanted as `text::draw` slants
        // it: the top to the right by its height times the slant.
        let corners: Vec<Point> = leaning.runs[0]
            .glyphs
            .iter()
            .filter_map(|glyph| {
                let ink = slanted.ink(glyph.id, 24.0)?;
                let (x, y) = (f64::from(glyph.x), f64::from(glyph.y));
                Some(
                    [
                        (ink.x0, ink.y0),
                        (ink.x0, ink.y1),
                        (ink.x1, ink.y0),
                        (ink.x1, ink.y1),
                    ]
                    .map(|(across, up)| Point::new(x + across + up * tan, y - up)),
                )
            })
            .flatten()
            .collect();
        let inside = |bounds: kurbo::Rect, point: &Point| {
            point.x >= bounds.x0 - 1e-3
                && point.x <= bounds.x1 + 1e-3
                && point.y >= bounds.y0 - 1e-3
                && point.y <= bounds.y1 + 1e-3
        };
        assert!(
            corners.iter().all(|point| inside(leaning.bounds, point)),
            "{:?} {corners:?}",
            leaning.bounds
        );
        // Upright text's bounds wouldn't hold it.
        assert!(!corners.iter().all(|point| inside(upright.bounds, point)));
        Ok(())
    }
}
