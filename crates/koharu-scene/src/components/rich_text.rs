//! Word styles inside one text box (fork issue #36, batch 3).
//!
//! Offsets count Unicode scalar values. Editing helpers preserve canonical
//! runs; callers with access to the displayed text also clamp to its length.

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

/// A field to return to the box's inherited style. This is an edit intent,
/// not part of the persisted component.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RunStyleField {
    Font,
    FontWeight,
    FontStyle,
    SizeScale,
    Color,
    FillGradient,
    StrokeColor,
    StrokeWidth,
    OuterStrokeColor,
    OuterStrokeWidth,
}

impl RunStyle {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    pub fn validate(&self) -> Result<()> {
        if self.is_empty()
            || self
                .font
                .as_ref()
                .is_some_and(|font| font.len() > 4096 || font.contains('\0'))
            || self
                .font_weight
                .is_some_and(|weight| !(1..=1000).contains(&weight))
            || self
                .size_scale
                .is_some_and(|scale| !scale.is_finite() || !(0.25..=4.0).contains(&scale))
            || [self.stroke_width, self.outer_stroke_width]
                .into_iter()
                .flatten()
                .any(|width| !width.is_finite() || width < 0.0)
        {
            return Err(Error::invalid("word style is invalid or empty"));
        }
        if let Some(gradient) = &self.fill_gradient
            && (!gradient.angle_degrees.is_finite()
                || !(2..=8).contains(&gradient.stops.len())
                || gradient
                    .stops
                    .iter()
                    .any(|stop| !stop.offset.is_finite() || !(0.0..=1.0).contains(&stop.offset))
                || gradient
                    .stops
                    .windows(2)
                    .any(|stops| stops[0].offset > stops[1].offset))
        {
            return Err(Error::invalid("word style gradient is invalid"));
        }
        Ok(())
    }

    fn patch(&mut self, set: &Self, clear: &[RunStyleField]) {
        macro_rules! set_fields {
            ($($field:ident),+ $(,)?) => {
                $(if let Some(value) = set.$field { self.$field = Some(value); })+
            };
        }
        if let Some(font) = &set.font {
            self.font = Some(font.clone());
        }
        if let Some(gradient) = &set.fill_gradient {
            self.fill_gradient = Some(gradient.clone());
        }
        set_fields!(
            font_weight,
            font_style,
            size_scale,
            color,
            stroke_color,
            stroke_width,
            outer_stroke_color,
            outer_stroke_width
        );
        for field in clear {
            match field {
                RunStyleField::Font => self.font = None,
                RunStyleField::FontWeight => self.font_weight = None,
                RunStyleField::FontStyle => self.font_style = None,
                RunStyleField::SizeScale => self.size_scale = None,
                RunStyleField::Color => self.color = None,
                RunStyleField::FillGradient => self.fill_gradient = None,
                RunStyleField::StrokeColor => self.stroke_color = None,
                RunStyleField::StrokeWidth => self.stroke_width = None,
                RunStyleField::OuterStrokeColor => self.outer_stroke_color = None,
                RunStyleField::OuterStrokeWidth => self.outer_stroke_width = None,
            }
        }
    }
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
        let mut previous: Option<&StyleRun> = None;
        for run in &self.runs {
            if run.start >= run.end || previous.is_some_and(|previous| run.start < previous.end) {
                return Err(Error::invalid(
                    "style runs must be non-empty, sorted and must not overlap",
                ));
            }
            if previous
                .is_some_and(|previous| previous.end == run.start && previous.style == run.style)
            {
                return Err(Error::invalid(
                    "equal neighbouring word styles must be merged",
                ));
            }
            run.style.validate()?;
            previous = Some(run);
        }
        self.origin.validate()
    }

    fn origin(&self) -> Option<&Origin> {
        Some(&self.origin)
    }

    fn set_origin(&mut self, origin: Origin) -> bool {
        self.origin = origin;
        true
    }
}

fn push(runs: &mut Vec<StyleRun>, run: StyleRun) {
    if run.start >= run.end || run.style.is_empty() {
        return;
    }
    if let Some(previous) = runs.last_mut()
        && previous.end == run.start
        && previous.style == run.style
    {
        previous.end = run.end;
        return;
    }
    runs.push(run);
}

/// Splits at the range edges, patches inherited gaps as well as existing runs,
/// and merges equal neighbours. `clear` wins if a field is also in `set`.
#[must_use]
pub fn apply_patch(
    runs: &[StyleRun],
    start: u32,
    end: u32,
    set: &RunStyle,
    clear: &[RunStyleField],
) -> Vec<StyleRun> {
    if start >= end {
        return runs.to_vec();
    }
    let mut result = Vec::new();
    let mut cursor = start;
    let patched = |start, end, mut style: RunStyle| {
        style.patch(set, clear);
        StyleRun { start, end, style }
    };
    for run in runs {
        if run.end <= start {
            push(&mut result, run.clone());
            continue;
        }
        if run.start >= end {
            if cursor < end {
                push(&mut result, patched(cursor, end, RunStyle::default()));
                cursor = end;
            }
            push(&mut result, run.clone());
            continue;
        }
        if run.start < start {
            push(
                &mut result,
                StyleRun {
                    end: start,
                    ..run.clone()
                },
            );
        }
        if cursor < run.start {
            push(&mut result, patched(cursor, run.start, RunStyle::default()));
        }
        let covered_end = run.end.min(end);
        push(
            &mut result,
            patched(run.start.max(start), covered_end, run.style.clone()),
        );
        cursor = covered_end;
        if run.end > end {
            push(
                &mut result,
                StyleRun {
                    start: end,
                    ..run.clone()
                },
            );
        }
    }
    if cursor < end {
        push(&mut result, patched(cursor, end, RunStyle::default()));
    }
    result
}

/// Removes every override in the range, preserving the styles on either side.
#[must_use]
pub fn clear_range(runs: &[StyleRun], start: u32, end: u32) -> Vec<StyleRun> {
    if start >= end {
        return runs.to_vec();
    }
    let mut result = Vec::new();
    for run in runs {
        if run.end <= start || run.start >= end {
            push(&mut result, run.clone());
        } else {
            push(
                &mut result,
                StyleRun {
                    end: start.min(run.end),
                    ..run.clone()
                },
            );
            push(
                &mut result,
                StyleRun {
                    start: end.max(run.start),
                    ..run.clone()
                },
            );
        }
    }
    result
}

/// Bounds stored runs to the displayed text before an edit or a read.
#[must_use]
pub fn clamp(runs: &[StyleRun], text_len: u32) -> Vec<StyleRun> {
    let mut result = Vec::new();
    for run in runs {
        push(
            &mut result,
            StyleRun {
                end: run.end.min(text_len),
                ..run.clone()
            },
        );
    }
    result
}

/// Remaps one known replacement. Inserted characters inherit the preceding
/// character's style, including at a run's end; position zero uses the first
/// character's style. Unstyled preceding characters retain inheritance.
#[must_use]
pub fn replace(runs: &[StyleRun], start: u32, end: u32, insert_len: u32) -> Vec<StyleRun> {
    if start > end {
        return runs.to_vec();
    }
    let inherited = runs.iter().find(|run| {
        if start == 0 {
            run.start == 0
        } else {
            run.start < start && start <= run.end
        }
    });
    let mut result = Vec::new();
    for run in runs.iter().take_while(|run| run.start < start) {
        push(
            &mut result,
            StyleRun {
                end: run.end.min(start),
                ..run.clone()
            },
        );
    }
    let inserted_end = start.saturating_add(insert_len);
    if let Some(run) = inherited {
        push(
            &mut result,
            StyleRun {
                start,
                end: inserted_end,
                style: run.style.clone(),
            },
        );
    }
    for run in runs.iter().filter(|run| run.end > end) {
        push(
            &mut result,
            StyleRun {
                start: inserted_end.saturating_add(run.start.max(end) - end),
                end: inserted_end.saturating_add(run.end - end),
                style: run.style.clone(),
            },
        );
    }
    result
}

/// Treats the non-matching middle, after a common prefix and suffix, as one
/// replacement. Prefix and suffix lengths are scalar values, never bytes.
#[must_use]
pub fn remap(runs: &[StyleRun], old_text: &str, new_text: &str) -> Vec<StyleRun> {
    let old = old_text.chars().collect::<Vec<_>>();
    let new = new_text.chars().collect::<Vec<_>>();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let runs = clamp(runs, old.len() as u32);
    let result = replace(
        &runs,
        prefix as u32,
        (old.len() - suffix) as u32,
        (new.len() - prefix - suffix) as u32,
    );
    clamp(&result, new.len() as u32)
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
    use super::*;

    fn bold() -> RunStyle {
        RunStyle {
            font_weight: Some(700),
            ..Default::default()
        }
    }

    fn red() -> RunStyle {
        RunStyle {
            color: Some([255, 0, 0, 255]),
            ..Default::default()
        }
    }

    fn run(start: u32, end: u32, style: RunStyle) -> StyleRun {
        StyleRun { start, end, style }
    }

    fn valid(runs: Vec<StyleRun>) -> bool {
        TextRuns {
            origin: Origin::User,
            runs,
        }
        .validate(&ValidationContext::new(&|_| true, &|_| true))
        .is_ok()
    }

    fn gradient() -> FillGradient {
        FillGradient {
            kind: super::super::GradientKind::Linear,
            angle_degrees: 45.0,
            stops: vec![
                super::super::GradientStop {
                    offset: 0.0,
                    color: [0, 0, 0, 255],
                },
                super::super::GradientStop {
                    offset: 1.0,
                    color: [255; 4],
                },
            ],
        }
    }

    #[test]
    fn rejects_invalid_run_shapes_and_equal_neighbours() {
        for runs in [
            vec![run(1, 1, bold())],
            vec![run(2, 1, bold())],
            vec![run(0, 3, bold()), run(2, 4, red())],
            vec![run(3, 4, bold()), run(0, 2, red())],
            vec![run(0, 2, bold()), run(2, 4, bold())],
            vec![run(0, 1, RunStyle::default())],
        ] {
            assert!(!valid(runs));
        }
        assert!(valid(vec![run(0, 2, bold()), run(3, 4, bold())]));
        assert!(valid(vec![run(0, 2, bold()), run(2, 4, red())]));
    }

    #[test]
    fn validates_word_style_values_and_gradient_like_typography() {
        let mut cases = vec![RunStyle::default()];
        for size_scale in [f32::NAN, f32::INFINITY, -1.0, 0.0, 0.249, 4.001] {
            cases.push(RunStyle {
                size_scale: Some(size_scale),
                ..Default::default()
            });
        }
        for font_weight in [0, 1001] {
            cases.push(RunStyle {
                font_weight: Some(font_weight),
                ..Default::default()
            });
        }
        for width in [f32::NAN, f32::INFINITY, -0.1] {
            cases.push(RunStyle {
                stroke_width: Some(width),
                ..Default::default()
            });
            cases.push(RunStyle {
                outer_stroke_width: Some(width),
                ..Default::default()
            });
        }
        cases.push(RunStyle {
            font: Some("a\0b".into()),
            ..Default::default()
        });
        cases.push(RunStyle {
            font: Some("x".repeat(4097)),
            ..Default::default()
        });
        for invalid in 0..6 {
            let mut fill_gradient = gradient();
            match invalid {
                0 => fill_gradient.angle_degrees = f32::INFINITY,
                1 => fill_gradient.stops.clear(),
                2 => fill_gradient.stops = vec![fill_gradient.stops[0].clone(); 9],
                3 => fill_gradient.stops[0].offset = f32::NAN,
                4 => fill_gradient.stops[0].offset = 1.1,
                _ => fill_gradient.stops.reverse(),
            }
            cases.push(RunStyle {
                fill_gradient: Some(fill_gradient),
                ..Default::default()
            });
        }
        for style in cases {
            assert!(style.validate().is_err(), "{style:?}");
        }
        for size_scale in [0.25, 1.0, 4.0] {
            assert!(
                RunStyle {
                    size_scale: Some(size_scale),
                    ..Default::default()
                }
                .validate()
                .is_ok()
            );
        }
        assert!(
            RunStyle {
                font_weight: Some(1000),
                stroke_width: Some(0.0),
                outer_stroke_width: Some(10000.0),
                fill_gradient: Some(gradient()),
                ..Default::default()
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn patch_splits_existing_styles_and_fills_unstyled_gaps() {
        let both = RunStyle {
            color: red().color,
            ..bold()
        };
        let runs = vec![run(2, 5, bold()), run(7, 10, bold())];
        assert_eq!(
            apply_patch(&runs, 3, 8, &red(), &[]),
            vec![
                run(2, 3, bold()),
                run(3, 5, both.clone()),
                run(5, 7, red()),
                run(7, 8, both),
                run(8, 10, bold()),
            ]
        );
        assert_eq!(
            apply_patch(&runs, 0, 12, &bold(), &[]),
            vec![run(0, 12, bold())]
        );
    }

    #[test]
    fn clearing_fields_preserves_other_fields_and_clear_wins() {
        let both = RunStyle {
            color: red().color,
            ..bold()
        };
        let runs = vec![run(0, 6, both.clone())];
        assert_eq!(
            apply_patch(
                &runs,
                2,
                4,
                &RunStyle::default(),
                &[RunStyleField::FontWeight]
            ),
            vec![run(0, 2, both.clone()), run(2, 4, red()), run(4, 6, both),]
        );
        assert!(
            apply_patch(
                &[run(0, 6, bold())],
                0,
                6,
                &bold(),
                &[RunStyleField::FontWeight]
            )
            .is_empty()
        );
        assert_eq!(
            clear_range(&runs, 2, 4),
            vec![
                run(0, 2, runs[0].style.clone()),
                run(4, 6, runs[0].style.clone())
            ]
        );
        assert!(clear_range(&runs, 0, 99).is_empty());
    }

    #[test]
    fn every_field_can_be_set_and_cleared() {
        let style = RunStyle {
            font: Some("Example".into()),
            font_weight: Some(600),
            font_style: Some(FontStyle::Italic),
            size_scale: Some(1.2),
            color: Some([1; 4]),
            fill_gradient: Some(gradient()),
            stroke_color: Some([2; 4]),
            stroke_width: Some(2.0),
            outer_stroke_color: Some([3; 4]),
            outer_stroke_width: Some(4.0),
        };
        let runs = apply_patch(&[], 0, 3, &style, &[]);
        assert_eq!(runs, vec![run(0, 3, style)]);
        assert!(
            apply_patch(
                &runs,
                0,
                3,
                &RunStyle::default(),
                &[
                    RunStyleField::Font,
                    RunStyleField::FontWeight,
                    RunStyleField::FontStyle,
                    RunStyleField::SizeScale,
                    RunStyleField::Color,
                    RunStyleField::FillGradient,
                    RunStyleField::StrokeColor,
                    RunStyleField::StrokeWidth,
                    RunStyleField::OuterStrokeColor,
                    RunStyleField::OuterStrokeWidth,
                ]
            )
            .is_empty()
        );
    }

    #[test]
    fn insertion_inherits_only_the_preceding_character_or_first_character_at_zero() {
        let runs = vec![run(2, 5, bold())];
        for (position, expected) in [
            (0, vec![run(4, 7, bold())]),
            (1, vec![run(4, 7, bold())]),
            (2, vec![run(4, 7, bold())]),
            (3, vec![run(2, 7, bold())]),
            (5, vec![run(2, 7, bold())]),
            (6, runs.clone()),
        ] {
            assert_eq!(
                replace(&runs, position, position, 2),
                expected,
                "position {position}"
            );
        }
        assert_eq!(
            replace(&[run(0, 3, bold())], 0, 0, 2),
            vec![run(0, 5, bold())]
        );
        assert_eq!(
            replace(&[run(0, 2, bold()), run(2, 4, red())], 2, 2, 1),
            vec![run(0, 3, bold()), run(3, 5, red())]
        );
    }

    #[test]
    fn deletion_shrinks_removes_and_merges_runs() {
        let runs = vec![run(1, 4, bold()), run(6, 9, bold())];
        assert_eq!(replace(&runs, 3, 7, 0), vec![run(1, 5, bold())]);
        assert_eq!(replace(&runs, 0, 6, 0), vec![run(0, 3, bold())]);
        assert!(replace(&runs, 0, 10, 0).is_empty());
    }

    #[test]
    fn remap_preserves_unicode_words_and_handles_whole_text_replacement() {
        let text = "aé👻magic!";
        let runs = vec![run(3, 8, bold())];
        for (new_text, expected) in [
            ("aé👻maXgic!", vec![run(3, 9, bold())]),
            ("aé👻magicX!", vec![run(3, 9, bold())]),
            ("Zaé👻magic!", vec![run(4, 9, bold())]),
            ("aé👻Xmagic!", vec![run(4, 9, bold())]),
            ("aé!", vec![]),
            ("other", vec![]),
            ("", vec![]),
            (text, runs.clone()),
        ] {
            assert_eq!(remap(&runs, text, new_text), expected, "{new_text}");
        }
        assert_eq!(
            remap(&[run(0, 3, bold())], "old", "new text"),
            vec![run(0, 8, bold())]
        );
    }

    #[test]
    fn bounded_small_edits_match_a_character_style_model() {
        let patterns = [
            vec![],
            vec![run(0, 6, bold())],
            vec![run(1, 3, bold()), run(4, 6, red())],
            vec![run(0, 2, bold()), run(2, 4, red()), run(4, 6, bold())],
        ];
        for runs in patterns {
            let per_char = (0..6)
                .map(|i| {
                    runs.iter()
                        .find(|r| r.start <= i && i < r.end)
                        .map(|r| r.style.clone())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>();
            for start in 0..=6 {
                for end in start..=6 {
                    for inserted in 0..=3 {
                        let inherited =
                            per_char[if start == 0 { 0 } else { start as usize - 1 }].clone();
                        let mut expected = per_char.clone();
                        expected.splice(
                            start as usize..end as usize,
                            vec![inherited; inserted as usize],
                        );
                        let actual = replace(&runs, start, end, inserted);
                        assert!(valid(actual.clone()));
                        let expanded = (0..expected.len() as u32)
                            .map(|i| {
                                actual
                                    .iter()
                                    .find(|r| r.start <= i && i < r.end)
                                    .map(|r| r.style.clone())
                                    .unwrap_or_default()
                            })
                            .collect::<Vec<_>>();
                        assert_eq!(expanded, expected, "{runs:?}, {start}..{end}, {inserted}");
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn explicit_style_reset_relinquishes_user_ownership_and_is_undoable() {
        use crate::{
            At, Generation, PageDraft, ProducerId, Session, TextLayout, TextLayoutKind, Typography,
        };
        let generation = Generation::new(ProducerId::new("dev.koharu.test.translation").unwrap());
        let mut typography = Typography {
            origin: Origin::User,
            preferred_font: None,
            font_weight: None,
            font_style: None,
            size: Some(72.0),
            auto_fit: false,
            color: None,
            stroke_color: None,
            stroke_width: None,
            alignment: None,
            writing_mode: None,
            extensions: Default::default(),
            stroke_method: None,
            stroke_position: None,
            stroke_join: None,
            outer_stroke_color: None,
            outer_stroke_width: None,
            line_spacing: None,
            letter_spacing: None,
            fill_gradient: None,
        };
        let mut session = Session::memory().await.unwrap();
        let mut layer = None;
        let patch = session
            .snapshot()
            .patch(|edit| {
                let page = edit.add_page(PageDraft::new("page", 100.0, 100.0), At::End)?;
                let content = edit.add_text_content(page, At::End)?;
                let id = edit.add_text_layer(
                    page,
                    At::End,
                    content,
                    &TextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: None,
                        warp: None,
                    },
                )?;
                edit.set(id, &typography)?;
                layer = Some(id);
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        let layer = layer.unwrap();
        let snapshot = session.snapshot();
        assert!(
            snapshot
                .patch(|edit| edit.reset_typography(layer, &typography))
                .is_err()
        );
        typography.origin = Origin::Generated(generation.clone());
        typography.size = None;
        typography.auto_fit = true;
        assert!(
            snapshot
                .edit_as(generation)
                .reset_typography(layer, &typography)
                .is_err()
        );
        let patch = snapshot
            .patch(|edit| edit.reset_typography(layer, &typography))
            .unwrap();
        let commit = session.commit(patch).await.unwrap();
        assert_eq!(
            session.snapshot().component::<Typography>(layer).unwrap(),
            Some(typography)
        );
        session.undo(commit.revision).await.unwrap();
        let restored = session
            .snapshot()
            .component::<Typography>(layer)
            .unwrap()
            .unwrap();
        assert_eq!(restored.origin, Origin::User);
        assert_eq!(restored.size, Some(72.0));
    }

    #[tokio::test]
    async fn clamped_runs_commit_and_survive_project_save_open() {
        use crate::{At, Authored, PageDraft, Session, SourceText, TextLayout, TextLayoutKind};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("word-styles.khrproj");
        let mut session = Session::create(&path).await.unwrap();
        let text = "é魔👻";
        let runs = TextRuns {
            origin: Origin::User,
            runs: clamp(&[run(1, 99, bold())], text.chars().count() as u32),
        };
        let mut layer = None;
        let patch = session
            .snapshot()
            .patch(|edit| {
                let page = edit.add_page(PageDraft::new("page", 100.0, 100.0), At::End)?;
                let content = edit.add_text_content(page, At::End)?;
                edit.set(
                    content,
                    &SourceText {
                        text: Authored::user(text.into()),
                        language: None,
                    },
                )?;
                let id = edit.add_text_layer(
                    page,
                    At::End,
                    content,
                    &TextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: None,
                        warp: None,
                    },
                )?;
                edit.set(id, &runs)?;
                layer = Some(id);
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        assert_eq!(runs.runs[0].end, 3);
        drop(session);
        let reopened = Session::open(&path).await.unwrap();
        assert_eq!(
            reopened
                .snapshot()
                .component::<TextRuns>(layer.unwrap())
                .unwrap(),
            Some(runs)
        );
    }

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
