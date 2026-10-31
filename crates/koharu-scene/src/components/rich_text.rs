//! Word styles inside one text box (fork issue #36, batch 3).
//!
//! The foundation only defines the stored shape. WO-11A owns the validation
//! rules beyond the basics below, the edit helpers and every command that
//! writes it; WO-11B/11C read it in the renderer.

use revision::revisioned;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    Error, Result,
    component::{Component, ValidationContext},
};

use super::{FillGradient, FontStyle, Origin};

/// Overrides for one run of characters. Every field is optional: an empty
/// field uses the box's `Typography`, which stays the base style.
#[revisioned(revision = 1)]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct RunStyle {
    pub font: Option<String>,
    pub font_weight: Option<u16>,
    pub font_style: Option<FontStyle>,
    /// Size as a multiple of the box's laid-out size (1.15 = 15% bigger), so
    /// auto-fit keeps working. The UI shows and accepts pixels and converts.
    pub size_scale: Option<f32>,
    pub color: Option<[u8; 4]>,
    pub fill_gradient: Option<FillGradient>,
    pub stroke_color: Option<[u8; 4]>,
    pub stroke_width: Option<f32>,
    pub outer_stroke_color: Option<[u8; 4]>,
    pub outer_stroke_width: Option<f32>,
}

/// One styled range. `start` and `end` count Unicode scalar values (Rust
/// `char`s) of the text the box shows: its translation, or its source when it
/// has no translation. `start < end`.
#[revisioned(revision = 1)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct StyleRun {
    pub start: u32,
    pub end: u32,
    pub style: RunStyle,
}

/// Word styles of a text layer. Runs are sorted, never overlap and are never
/// empty. A box without this component (every project before batch 3) has
/// no word styles and renders exactly as before.
#[revisioned(revision = 1)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct TextRuns {
    pub origin: Origin,
    pub runs: Vec<StyleRun>,
}

impl Component for TextRuns {
    const KIND: &'static str = "dev.koharu.text.style_runs";

    fn validate(&self, _context: &ValidationContext<'_>) -> Result<()> {
        let mut previous_end = 0;
        for run in &self.runs {
            if run.start >= run.end || run.start < previous_end {
                return Err(Error::invalid(
                    "style runs must be non-empty, sorted and must not overlap",
                ));
            }
            previous_end = run.end;
        }
        self.origin.validate()
    }
}

/// Converts a range of `char` offsets (as stored in [`StyleRun`]) into byte
/// offsets of `text`. Offsets past the end clamp to the end of the text.
#[must_use]
pub fn char_range_to_byte_range(text: &str, start: u32, end: u32) -> std::ops::Range<usize> {
    let byte = |index: u32| {
        text.char_indices()
            .nth(index as usize)
            .map_or(text.len(), |(offset, _)| offset)
    };
    byte(start)..byte(end.max(start))
}

#[cfg(test)]
mod tests {
    use super::char_range_to_byte_range;

    #[test]
    fn char_ranges_map_to_byte_ranges_for_multibyte_text() {
        let text = "aé魔王!";
        assert_eq!(char_range_to_byte_range(text, 0, 1), 0..1);
        assert_eq!(char_range_to_byte_range(text, 1, 2), 1..3);
        assert_eq!(char_range_to_byte_range(text, 2, 4), 3..9);
        assert_eq!(char_range_to_byte_range(text, 4, 99), 9..10);
        assert_eq!(char_range_to_byte_range(text, 7, 3), 10..10);
    }
}
