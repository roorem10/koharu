//! Size steps: words made a step bigger or smaller, as the type bar's A+
//! and A− make them, the canvas's typing, the words edit and the agent
//! alike. A step changes each character's own size, so words of different
//! sizes keep their order and about their ratio, never all set to one size.

use std::ops::Range;

use koharu_scene::{CharStyle, RichText};

use crate::emphasis;

/// The size the renderer draws a text with none at, in pixels.
pub const SIZE: f64 = 24.0;

/// The smallest and largest sizes a step gives, in pixels, the type bar's
/// size field's bounds.
const SIZES: std::ops::RangeInclusive<f64> = 0.5..=300.0;

/// How much a step scales a size by.
const FACTOR: f64 = 1.05;

/// What a step rounds a size to, in pixels, and the least it moves one by.
const GRAIN: f64 = 0.5;

/// `size` a step bigger or smaller: 5% more or less, rounded to the nearest
/// half pixel and moved by at least half a pixel, so a small size never
/// stalls, within 0.5 to 300 pixels. A size that isn't finite steps from
/// 24 pixels, and one outside the bounds from the nearest.
pub fn stepped(size: f64, bigger: bool) -> f64 {
    let (min, max) = (*SIZES.start(), *SIZES.end());
    let size = match size.is_finite() {
        true => size,
        false => SIZE,
    }
    .clamp(min, max);
    let scaled = match bigger {
        true => size * FACTOR,
        false => size / FACTOR,
    };
    let rounded = (scaled / GRAIN).round() * GRAIN;
    let moved = match bigger {
        true => rounded.max(size + GRAIN),
        false => rounded.min(size - GRAIN),
    };
    moved.clamp(min, max)
}

/// `marks`, a character's, with its size a step bigger or smaller
/// ([`stepped`]) from its own, else `base`, the text's: no size of its own
/// where that comes out as the text's, as [`emphasis::patch`] writes none,
/// so a step back undoes a step. Its other marks stay.
pub fn stepped_marks(marks: &CharStyle, base: f64, bigger: bool) -> CharStyle {
    CharStyle {
        size: Some(stepped(marks.size.unwrap_or(base), bigger)).filter(|size| *size != base),
        ..marks.clone()
    }
}

/// `content` with each character in `range`, UTF-8 byte offsets, a step
/// bigger or smaller from its own size, else `base` ([`stepped_marks`]), so
/// words of different sizes keep their order and about their ratio; none
/// for an empty range, or one past the text, backwards, or off a character
/// boundary.
pub fn resized(
    content: &RichText,
    range: Range<usize>,
    base: f64,
    bigger: bool,
) -> Option<RichText> {
    if range.is_empty() {
        return None;
    }
    emphasis::restyled(content, range, |marks| stepped_marks(marks, base, bigger))
}

#[cfg(test)]
mod tests {
    use koharu_scene::Span;

    use super::*;

    fn sized(insert: &str, size: Option<f64>) -> Span {
        Span {
            insert: insert.to_owned(),
            attributes: CharStyle {
                size,
                ..CharStyle::default()
            },
        }
    }

    #[test]
    fn a_step_is_five_percent_to_the_half_pixel() {
        assert_eq!(stepped(24.0, true), 25.0);
        assert_eq!(stepped(24.0, false), 23.0);
        assert_eq!(stepped(40.0, true), 42.0);
        assert_eq!(stepped(40.0, false), 38.0);
        // An emphasis size, to a hundredth, comes back to the grid.
        assert_eq!(stepped(27.6, true), 29.0);
    }

    #[test]
    fn small_sizes_never_stall() {
        let mut size = 4.0;
        for _ in 0..40 {
            let next = stepped(size, true);
            assert!(next >= size + 0.5, "{size} went to {next}");
            size = next;
        }
        let mut size = 4.0;
        for _ in 0..7 {
            let next = stepped(size, false);
            assert!(next <= size - 0.5, "{size} went to {next}");
            size = next;
        }
        assert_eq!(size, 0.5);
    }

    #[test]
    fn a_step_stays_within_half_a_pixel_and_three_hundred() {
        assert_eq!(stepped(0.5, false), 0.5);
        assert_eq!(stepped(300.0, true), 300.0);
        assert_eq!(stepped(299.0, true), 300.0);
        // A size outside the bounds steps from the nearest.
        assert_eq!(stepped(0.1, true), 1.0);
        assert_eq!(stepped(1000.0, false), 285.5);
        assert_eq!(stepped(1000.0, true), 300.0);
    }

    #[test]
    fn a_size_that_isnt_finite_steps_from_twenty_four() {
        assert_eq!(stepped(f64::NAN, true), 25.0);
        assert_eq!(stepped(f64::INFINITY, false), 23.0);
        assert_eq!(stepped(f64::NEG_INFINITY, true), 25.0);
    }

    #[test]
    fn each_character_steps_from_its_own_size() {
        // "Big" at 40 in a text of 20, then 魔王, six bytes, of the text's.
        let content = RichText(vec![sized("Big", Some(40.0)), sized(" 魔王", None)]);
        assert_eq!(
            resized(&content, 0..content.text().len(), 20.0, true),
            Some(RichText(vec![
                sized("Big", Some(42.0)),
                sized(" 魔王", Some(21.0))
            ]))
        );
        assert_eq!(
            resized(&content, 0..content.text().len(), 20.0, false),
            Some(RichText(vec![
                sized("Big", Some(38.0)),
                sized(" 魔王", Some(19.0))
            ]))
        );
        // Only the range steps: "ig" and the space.
        assert_eq!(
            resized(&content, 1..4, 20.0, true),
            Some(RichText(vec![
                sized("B", Some(40.0)),
                sized("ig", Some(42.0)),
                sized(" ", Some(21.0)),
                sized("魔王", None),
            ]))
        );
    }

    #[test]
    fn a_characters_other_marks_stay() {
        let bold = CharStyle {
            weight: Some(700),
            ..CharStyle::default()
        };
        assert_eq!(
            stepped_marks(&bold, 24.0, true),
            CharStyle {
                size: Some(25.0),
                ..bold.clone()
            }
        );
        let sized = CharStyle {
            size: Some(25.0),
            ..bold.clone()
        };
        assert_eq!(stepped_marks(&sized, 24.0, false), bold);
    }

    #[test]
    fn a_step_back_to_the_texts_own_size_leaves_no_mark() {
        let plain = RichText::from("Hello");
        let bigger = resized(&plain, 0..5, 24.0, true).expect("a range");
        assert_eq!(bigger, RichText(vec![sized("Hello", Some(25.0))]));
        assert_eq!(resized(&bigger, 0..5, 24.0, false), Some(plain));
    }

    #[test]
    fn a_range_holding_no_whole_character_is_refused() {
        let content = RichText::from("魔王");
        assert_eq!(resized(&content, 1..3, 24.0, true), None);
        assert_eq!(resized(&content, 3..3, 24.0, true), None);
        assert_eq!(resized(&content, 3..9, 24.0, true), None);
        assert_eq!(
            resized(&content, 3..6, 24.0, true).map(|text| text.0.len()),
            Some(2)
        );
    }
}
