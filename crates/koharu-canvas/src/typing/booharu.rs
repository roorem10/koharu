//! Booharu's part of typing: marks taken off the characters being typed for
//! real, as the type bar's Clear, B off and I off ask, a step bigger or
//! smaller for them, A+ and A−, and the selected text the type bar quotes,
//! "Applies to". A child of the typing module, as it reads the session's
//! own fields, the way koharu-pipeline's scheduler keeps its run policy in
//! `scheduler/booharu.rs`. Each change holds a style at a caret and relays
//! a selection as `Typing::style` does, and the commit stays Koharu's one
//! step, so a content back to the one the session found commits nothing.

use anyhow::Context as _;
use booharu_core::{emphasis, sizes};
use icu_segmenter::GraphemeClusterSegmenter;
use koharu_scene::CharStyle;

use super::Typing;
use crate::booharu::Unmark;

/// The weight a text with none is drawn at, and the one B off writes over
/// a bold text.
const REGULAR: u16 = 400;

/// The lightest weight a text counts as bold at, as B shows pressed.
const BOLD_FROM: u16 = 600;

/// The most grapheme clusters of the selection the type bar quotes.
const QUOTED: usize = 80;

/// The characters a line breaks at, as the renderer breaks one: Unicode's
/// mandatory breaks.
const LINE_BREAKS: [char; 7] = [
    '\n', '\r', '\u{000B}', '\u{000C}', '\u{0085}', '\u{2028}', '\u{2029}',
];

impl Typing {
    /// Takes marks off the selected characters, as `what` says: every mark,
    /// or the weight or the italic mark, or, where the text itself is bold
    /// or italic, regular or upright in its place, as the words edit takes
    /// bold and italic off. The text is its base, a block's source while it
    /// has no translation. At a caret, takes them off the style held for
    /// what's typed next.
    pub fn unmark(&mut self, what: Unmark) -> anyhow::Result<()> {
        let own = &self.base.style.chars;
        let weight = (own.weight.unwrap_or(REGULAR) >= BOLD_FROM).then_some(REGULAR);
        let italic = (own.italic == Some(true)).then_some(false);
        let unmarked = move |marks: &CharStyle| match what {
            Unmark::All => CharStyle::default(),
            Unmark::Bold => CharStyle {
                weight,
                ..marks.clone()
            },
            Unmark::Italic => CharStyle {
                italic,
                ..marks.clone()
            },
        };
        let range = self.selected();
        if range.is_empty() {
            self.held = Some(unmarked(&self.marks()));
            self.touch();
            return Ok(());
        }
        self.content = emphasis::restyled(&self.content, range, unmarked)
            .context("the selection isn't whole characters of the text")?;
        self.relay()
    }

    /// The selected characters a step bigger or smaller, each from its own
    /// size, else the text's, 24 pixels without one ([`sizes::resized`]),
    /// so words of different sizes keep their order; at a caret, the size
    /// held for what's typed next, stepped from the marks there. The text's
    /// fit stays, so a box that fits its text keeps fitting it.
    pub fn step_size(&mut self, bigger: bool) -> anyhow::Result<()> {
        let base = self.base.style.chars.size.unwrap_or(sizes::SIZE);
        let range = self.selected();
        if range.is_empty() {
            self.held = Some(sizes::stepped_marks(&self.marks(), base, bigger));
            self.touch();
            return Ok(());
        }
        self.content = sizes::resized(&self.content, range, base, bigger)
            .context("the selection isn't whole characters of the text")?;
        self.relay()
    }

    /// The selected characters as the type bar quotes them: each line break
    /// a space, cut to 80 grapheme clusters with "…" after; none at a caret,
    /// nor for a selection that isn't the content's own characters, as one
    /// a drag over the input method's composition leaves, placed by the
    /// text shown with the composition in it.
    pub(super) fn quoted(&self) -> Option<String> {
        let range = self.selected();
        if range.is_empty() {
            return None;
        }
        let text = self
            .content
            .text()
            .get(range)?
            .replace("\r\n", " ")
            .replace(LINE_BREAKS, " ");
        let cut = GraphemeClusterSegmenter::new()
            .segment_str(&text)
            .nth(QUOTED)
            .filter(|&end| end < text.len());
        Some(match cut {
            Some(end) => format!("{}…", &text[..end]),
            None => text,
        })
    }
}

#[cfg(test)]
mod tests {
    use koharu_scene::{CharStyle, Gradient, GradientKind, GradientStop, Rgba};

    use super::super::style::shared;
    use crate::CharKey;

    /// A two-stop linear gradient from black to `to`.
    fn gradient(to: Rgba) -> Gradient {
        let stop = |offset, color| GradientStop { offset, color };
        Gradient {
            kind: GradientKind::Linear,
            angle: 90.0,
            stops: vec![stop(0.0, Rgba([0.0, 0.0, 0.0, 1.0])), stop(1.0, to)],
        }
    }

    #[test]
    fn a_gradient_or_letter_spacing_the_characters_dont_share_shows_mixed() {
        let red = Rgba([1.0, 0.0, 0.0, 1.0]);
        let style = |tracking| CharStyle {
            size: Some(32.0),
            fill_gradient: Some(gradient(red)),
            tracking,
            ..CharStyle::default()
        };
        // Shared, they show as they are.
        let (chars, mixed) = shared([style(Some(200.0)), style(Some(200.0))].into_iter());
        assert_eq!((chars, mixed), (style(Some(200.0)), Vec::new()));
        // Letter spacing on some characters only is mixed, and left empty.
        let (chars, mixed) = shared([style(Some(200.0)), style(None)].into_iter());
        assert_eq!((chars, mixed), (style(None), vec![CharKey::Tracking]));
        // So is another gradient, or none, beside a gradient.
        let blue = CharStyle {
            fill_gradient: Some(gradient(Rgba([0.0, 0.0, 1.0, 1.0]))),
            ..style(None)
        };
        let plain = CharStyle {
            fill_gradient: None,
            ..style(None)
        };
        for other in [blue, plain] {
            let (chars, mixed) = shared([style(None), other].into_iter());
            assert_eq!(chars.fill_gradient, None);
            assert_eq!(chars.size, Some(32.0));
            assert_eq!(mixed, vec![CharKey::FillGradient]);
        }
    }
}
