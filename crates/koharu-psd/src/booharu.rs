//! Booharu's part of PSD export: the rules a text follows on its way into a
//! layer. A type layer's effects hold one border, one second border, and
//! one gradient fill for all its words (`effects`), so a text whose words
//! differ in them can't keep them as effects (`word_effects`); a layer
//! whose effects Photoshop draws keeps its glyphs alone in its pixels
//! (`glyphs_only`).

use koharu_renderer::ResolvedText;
use photocraft_psd::TaggedBlock;

pub(crate) use self::effects::effects;

mod effects;

/// Whether `text`'s words differ in their effects: its runs don't all share
/// one border, one second border, and one gradient fill spread over one
/// box, which one type layer's effects can't hold. Their fonts, sizes,
/// colours, faked bold and slant, and letter spacing may differ, as a type
/// layer's style runs hold those.
pub(crate) fn word_effects(text: &ResolvedText) -> bool {
    let Some(first) = text.runs.first() else {
        return false;
    };
    text.runs.iter().any(|run| {
        run.stroke != first.stroke
            || run.outer_stroke != first.outer_stroke
            || run.fill_gradient != first.fill_gradient
            || run.fill_extent != first.fill_extent
    })
}

/// Whether a layer with `blocks` keeps its glyphs alone in its pixels, with
/// no border: a type layer, as Photoshop draws its pixels again from its
/// text, and a layer with effects, as Photoshop draws those around them.
/// Borders in the pixels would show twice under an effect, and where no
/// effect draws them would leave a type layer as soon as it's edited.
pub(crate) fn glyphs_only(blocks: &[TaggedBlock]) -> bool {
    blocks
        .iter()
        .any(|block| matches!(&block.key, b"TySh" | b"lfx2" | b"lmfx"))
}

/// A run over `range` in black 24-pixel type with no effects.
#[cfg(test)]
pub(crate) fn plain_run(range: std::ops::Range<usize>) -> koharu_renderer::TextRun {
    koharu_renderer::TextRun {
        range,
        post_script_name: "Sans".to_owned(),
        size: 24.0,
        fill: koharu_scene::Color::new([0.0, 0.0, 0.0, 1.0]),
        fill_gradient: None,
        fill_extent: None,
        stroke: None,
        outer_stroke: None,
        faux_bold: false,
        faux_italic: false,
        tracking: 0.0,
    }
}

/// A text laid out as `runs`, on one line at the origin.
#[cfg(test)]
pub(crate) fn resolved(runs: Vec<koharu_renderer::TextRun>) -> ResolvedText {
    ResolvedText {
        runs,
        frame: kurbo::Rect::new(0.0, 0.0, 200.0, 30.0),
        scale: 1.0,
        hyphens: Vec::new(),
        overflowed: false,
        baseline: 24.0,
        align: koharu_scene::Align::Start,
        rtl: false,
        line_height: 1.2,
        uncovered: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use koharu_renderer::TextRun;
    use koharu_scene::{Color, Gradient, GradientKind, GradientStop, Rgba};
    use kurbo::Rect;

    use super::*;

    const WHITE: Color = Color::new([1.0, 1.0, 1.0, 1.0]);
    const BLACK: Color = Color::new([0.0, 0.0, 0.0, 1.0]);

    /// A gradient from red to blue.
    fn gradient() -> Gradient {
        Gradient {
            kind: GradientKind::Linear,
            angle: 0.0,
            stops: [(0.0, [1.0, 0.0, 0.0, 1.0]), (1.0, [0.0, 0.0, 1.0, 1.0])]
                .into_iter()
                .map(|(offset, color)| GradientStop {
                    offset,
                    color: Rgba(color),
                })
                .collect(),
        }
    }

    /// Three runs of words, each changed by `change`, given its index.
    fn words(change: impl Fn(usize, &mut TextRun)) -> ResolvedText {
        let runs = (0..3)
            .map(|index| {
                let mut run = plain_run(index * 5..index * 5 + 5);
                change(index, &mut run);
                run
            })
            .collect();
        resolved(runs)
    }

    /// A border and second border, and a gradient spread over `extent`.
    fn effects(run: &mut TextRun, extent: Rect) {
        run.stroke = Some((WHITE, 4.0));
        run.outer_stroke = Some((BLACK, 2.0));
        run.fill_gradient = Some(gradient());
        run.fill_extent = Some(extent);
    }

    const TEXT: Rect = Rect::new(0.0, 2.0, 180.0, 30.0);
    const WORD: Rect = Rect::new(60.0, 2.0, 120.0, 30.0);

    #[test]
    fn words_differ_in_their_effects_only_by_border_or_gradient() {
        // One run shares its effects with itself, and runs differing in all
        // but their effects share them.
        let mut one = plain_run(0..15);
        effects(&mut one, TEXT);
        assert!(!word_effects(&resolved(vec![one])));
        assert!(!word_effects(&words(|index, run| {
            effects(run, TEXT);
            run.post_script_name = format!("Face{index}");
            run.size = 24.0 + index as f32;
            run.fill = Color::new([index as f32 / 3.0, 0.0, 0.0, 1.0]);
            run.faux_bold = index == 1;
            run.faux_italic = index == 2;
            run.tracking = index as f32 * 50.0;
        })));
        // A border or a second border of its own sets a word apart.
        assert!(word_effects(&words(|index, run| {
            effects(run, TEXT);
            if index == 1 {
                run.stroke = Some((BLACK, 4.0));
            }
        })));
        assert!(word_effects(&words(|index, run| {
            effects(run, TEXT);
            if index == 2 {
                run.outer_stroke = Some((BLACK, 3.0));
            }
        })));
        // So does a flat word in a gradient text, and a word given the
        // text's gradient as its own, which spreads it over its own ink.
        assert!(word_effects(&words(|index, run| {
            effects(run, TEXT);
            if index == 1 {
                run.fill_gradient = None;
                run.fill_extent = None;
            }
        })));
        assert!(word_effects(&words(|index, run| {
            effects(run, if index == 1 { WORD } else { TEXT });
        })));
        // Words apart given one gradient each spread it over their own.
        assert!(word_effects(&words(|index, run| {
            effects(run, TEXT + kurbo::Vec2::new(index as f64 * 60.0, 0.0));
        })));
    }

    #[test]
    fn a_type_layer_or_a_layer_with_effects_keeps_its_glyphs_alone() {
        let blocks = |keys: &[&[u8; 4]]| -> Vec<TaggedBlock> {
            keys.iter()
                .map(|key| TaggedBlock::new(**key, Vec::new()))
                .collect()
        };
        assert!(glyphs_only(&blocks(&[b"luni", b"TySh"])));
        assert!(glyphs_only(&blocks(&[b"lfx2"])));
        assert!(glyphs_only(&blocks(&[b"lmfx"])));
        assert!(!glyphs_only(&blocks(&[b"luni", b"vmsk", b"SoCo"])));
        assert!(!glyphs_only(&[]));
    }
}
