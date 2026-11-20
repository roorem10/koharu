//! Styling words by hand: a run of a text's characters made bold or italic,
//! stressed as its lettering category's emphasis says, a step bigger or
//! smaller, given values of its own, or cleared, as one undo step labelled
//! [`LABEL`], which the desktop's command and the agent's tool share
//! ([`style_words`]); every word style taken off whole boxes at once, as one
//! undo step labelled [`CLEAR_LABEL`] ([`clear_all`]); and the marks the
//! emphasis star gives text typed on the canvas ([`emphasis_patch`]).
//!
//! The words are UTF-8 byte offsets into the text of a block's translation
//! or of a text layer. The edit writes the text's spans whole, as Koharu's
//! rich editor does, and leaves the hand-edit record as it is: a mark set by
//! hand is a hand edit, and so is a change to one a run's emphasis set
//! (`booharu_core::edited`).

use std::{cell::Cell, collections::BTreeSet, ops::Range};

use anyhow::{Result, bail, ensure};
use booharu_core::{
    emphasis::{self, EmphasisSettings},
    lettering::{self, LetteringCategory},
    resolver,
    settings::LetteringSettings,
    sizes,
};
use koharu_scene::{CharStyle, Content, Edit, LayerId, Op, Project, RichText, Text};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{boxes::unlocked, styles::locked};

/// The undo step's label, which the History panel translates.
pub const LABEL: &str = "booharu_style_words";

/// The label of [`clear_all`]'s undo step, which the History panel
/// translates.
pub const CLEAR_LABEL: &str = "booharu_clear_word_styles";

/// The weight bold sets.
const BOLD: u16 = 700;
/// The weight bold off sets in a bold text.
const REGULAR: u16 = 400;
/// The lightest weight a text counts as bold at.
const BOLD_FROM: u16 = 600;

/// What a words edit does to its words: exactly one of `clear`, `emphasis`,
/// `step`, or the explicit values, `set`, `bold` and `italic`, which may go
/// together.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default, deny_unknown_fields)]
pub struct WordChange {
    /// The values the words get, each one set written over theirs.
    pub set: CharStyle,
    /// Bold, weight 700; false takes the words' weight off, or sets regular,
    /// 400, where the text itself is bold (600 or more). Not with a weight
    /// in `set`.
    pub bold: Option<bool>,
    /// Italic; false takes the words' italic off, or sets upright where the
    /// text itself is italic. Not with an italic in `set`.
    pub italic: Option<bool>,
    /// Stressed as `[booharu-emphasis]` says for the layer's lettering
    /// category, a text layer's as the default emphasis.
    pub emphasis: bool,
    /// Every value of the words' own taken off, so they look as the text.
    pub clear: bool,
    /// Each of the words a step bigger or smaller from its own size, else
    /// the text's, 24 pixels without one, as the type bar's A+ and A− step
    /// them, so words of different sizes keep their order.
    pub step: Option<SizeStep>,
}

/// A step a words edit makes its words bigger or smaller by: 5%, to the
/// half pixel, as `booharu_core::sizes` steps a size.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SizeStep {
    Bigger,
    Smaller,
}

/// What a [`WordChange`] asks for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Clear,
    Emphasis,
    /// A step, bigger when true.
    Step(bool),
    Explicit,
}

impl WordChange {
    fn mode(&self) -> Result<Mode> {
        let explicit =
            self.set != CharStyle::default() || self.bold.is_some() || self.italic.is_some();
        let mode = match (self.clear, self.emphasis, self.step, explicit) {
            (true, false, None, false) => Mode::Clear,
            (false, true, None, false) => Mode::Emphasis,
            (false, false, Some(step), false) => Mode::Step(step == SizeStep::Bigger),
            (false, false, None, true) => Mode::Explicit,
            _ => bail!("Choose clear, emphasis, step, or explicit style fields."),
        };
        ensure!(
            self.set.weight.is_none() || self.bold.is_none(),
            "Choose bold or a weight, not both."
        );
        ensure!(
            self.set.italic.is_none() || self.italic.is_none(),
            "Choose italic once: in set or as italic."
        );
        Ok(mode)
    }
}

/// The edit styling the characters `range`, UTF-8 byte offsets, of the text
/// of `layer` in `project`, a block's translation or a text layer's, as
/// `change` says, the emphasis by `emphasis` and the block's category by
/// `lettering`, which only `change.emphasis` reads, as one undo step
/// labelled [`LABEL`] holding one `SetRichText`; none when nothing changes.
/// With `expected`, the plain text the range was taken from, a text that has
/// changed since is refused.
///
/// Refused: a change asking for none or more than one of clear, emphasis,
/// a step and explicit values, or for a weight and bold or italic twice; a layer
/// that's neither a block nor a text layer; one that is, or is inside, a
/// locked layer ([`crate::boxes::Refusal::Locked`]); a block without a
/// translation; and an empty range, or one past the text or off a
/// character boundary.
pub fn style_words(
    project: &Project,
    layer: LayerId,
    range: Range<usize>,
    expected: Option<&str>,
    change: &WordChange,
    emphasis: &EmphasisSettings,
    lettering: &LetteringSettings,
) -> Result<Option<Edit>> {
    let mode = change.mode()?;
    let text = text(project, layer)?;
    let plain = text.content.text();
    if let Some(expected) = expected {
        ensure!(
            expected == plain,
            "The text changed. Select the words again."
        );
    }
    ensure!(
        range.start < range.end,
        "Select the words to style: the range {}..{} holds none",
        range.start,
        range.end
    );
    ensure!(
        plain.is_char_boundary(range.start) && plain.is_char_boundary(range.end),
        "The range {}..{} isn't whole characters of the text, which is {} bytes long",
        range.start,
        range.end,
        plain.len()
    );
    let own = &text.style.chars;
    let stressed = match mode {
        Mode::Emphasis => {
            let category = category(project, layer, lettering)?;
            emphasis::patch(&emphasis.style_for(category), own)
        }
        Mode::Clear | Mode::Step(_) | Mode::Explicit => CharStyle::default(),
    };
    let base = own.size.unwrap_or(sizes::SIZE);
    let bold = change.bold.map(|bold| match bold {
        true => Some(BOLD),
        false if own.weight.is_some_and(|weight| weight >= BOLD_FROM) => Some(REGULAR),
        false => None,
    });
    let italic = change.italic.map(|italic| match italic {
        true => Some(true),
        false if own.italic == Some(true) => Some(false),
        false => None,
    });
    let restyle = |marks: &CharStyle| match mode {
        Mode::Clear => CharStyle::default(),
        Mode::Emphasis => stressed.or(marks),
        Mode::Step(bigger) => sizes::stepped_marks(marks, base, bigger),
        Mode::Explicit => {
            let mut marks = change.set.or(marks);
            if let Some(weight) = bold {
                marks.weight = weight;
            }
            if let Some(italic) = italic {
                marks.italic = italic;
            }
            marks
        }
    };
    let changed = Cell::new(false);
    let restyled = emphasis::restyled(&text.content, range, |marks| {
        let new = restyle(marks);
        changed.set(changed.get() || new != *marks);
        new
    });
    let Some(restyled) = restyled else {
        bail!("the words can't be cut from the text");
    };
    Ok(changed.get().then(|| Edit {
        label: LABEL.to_owned(),
        ops: vec![Op::SetRichText {
            layer: layer.into(),
            spans: restyled.0,
        }],
    }))
}

/// What `clear_all` does to the boxes named: its edit, none when nothing
/// changes, with how many texts it clears and how many it leaves alone as
/// locked, which the desktop answers without the edit, which it applies.
#[derive(Clone, Debug, Default, Serialize, specta::Type)]
pub struct ClearedWords {
    /// The edit, one undo step labelled [`CLEAR_LABEL`]; never in the
    /// answer.
    #[serde(skip)]
    pub edit: Option<Edit>,
    /// Texts whose word styles it takes off.
    pub cleared: usize,
    /// Texts with word styles that it leaves as they are because they, or
    /// a layer holding them, are locked.
    pub locked: usize,
}

/// The edit taking every word style off the texts of `layers` in `project`,
/// each a block's translation or a text layer's, so their words look as
/// their texts, as Clear takes them off chosen words: one undo step
/// labelled [`CLEAR_LABEL`] holding one `SetRichText` for each text whose
/// spans carry any mark, its words then one unmarked span, with how many
/// texts it clears and leaves alone as locked; none when nothing changes. A
/// text that is, or is inside, a locked layer is left out and counted; a
/// text without marks, a block without a translation, and any other kind of
/// layer are left out. The hand-edit record stays as it is, as
/// [`style_words`] leaves it.
pub fn clear_all(project: &Project, layers: &[LayerId]) -> Result<ClearedWords> {
    let mut cleared = ClearedWords::default();
    let mut ops = Vec::new();
    let mut seen = BTreeSet::new();
    for &id in layers {
        if !seen.insert(id) {
            continue;
        }
        let layer = project.layer(id)?;
        let text = match &layer.content {
            Content::Block(block) => block.translation.as_ref(),
            Content::Text(text) => Some(&**text),
            _ => None,
        };
        let marked = |text: &&Text| {
            text.content
                .0
                .iter()
                .any(|span| span.attributes != CharStyle::default())
        };
        let Some(text) = text.filter(marked) else {
            continue;
        };
        if locked(project, layer)? {
            cleared.locked += 1;
            continue;
        }
        cleared.cleared += 1;
        ops.push(Op::SetRichText {
            layer: id.into(),
            spans: RichText::from(text.content.text().as_str()).0,
        });
    }
    cleared.edit = (!ops.is_empty()).then(|| Edit {
        label: CLEAR_LABEL.to_owned(),
        ops,
    });
    Ok(cleared)
}

/// The emphasis and lettering settings for a words edit, through `read`,
/// which reads them as the app holds them, only when it stresses the words
/// (`emphasis`): the other changes set the values they name, so a section
/// that can't be read never stops them, and they take the defaults, which
/// [`style_words`] doesn't read.
pub fn settings(
    emphasis: bool,
    read: impl FnOnce() -> Result<(EmphasisSettings, LetteringSettings)>,
) -> Result<(EmphasisSettings, LetteringSettings)> {
    match emphasis {
        true => read(),
        false => Ok(Default::default()),
    }
}

/// The marks the emphasis star gives text typed into `layer` in `project`,
/// a block or a text layer: the emphasis `emphasis` gives its lettering
/// category, as `lettering` reads it, a text layer's the default, over its
/// text's own character values, or for a block not yet translated the ones
/// Koharu's placement would give its translation. Refused as
/// [`style_words`] refuses the layer.
pub fn emphasis_patch(
    project: &Project,
    layer: LayerId,
    emphasis: &EmphasisSettings,
    lettering: &LetteringSettings,
) -> Result<CharStyle> {
    let own = match words(project, layer)? {
        Some(text) => text.style.chars.clone(),
        None => {
            let (page, _) = project.parent(layer)?;
            match &project.layer(layer)?.content {
                Content::Block(block) => resolver::koharu_chars(page, block, None),
                _ => CharStyle::default(),
            }
        }
    };
    let category = category(project, layer, lettering)?;
    Ok(emphasis::patch(&emphasis.style_for(category), &own))
}

/// The text of `layer` in `project` whose words [`style_words`] styles, a
/// block's translation or a text layer's, for a caller that finds the words
/// in it first, as the agent's tool finds them by their wording or by
/// character offsets. Refused as [`style_words`] refuses the layer: one
/// that's neither a block nor a text layer, one that is, or is inside, a
/// locked layer, and a block without a translation.
pub fn text(project: &Project, layer: LayerId) -> Result<&Text> {
    match words(project, layer)? {
        Some(text) => Ok(text),
        None => bail!("block {layer} has no translation, so it has no words to style"),
    }
}

/// The text of `layer` in `project` whose words are styled, none for a
/// block without a translation. Refuses any other layer, and a locked one.
fn words(project: &Project, layer: LayerId) -> Result<Option<&Text>> {
    let words = match &project.layer(layer)?.content {
        Content::Block(block) => block.translation.as_ref(),
        Content::Text(text) => Some(&**text),
        _ => bail!("layer {layer} is neither a block nor a text, so it has no words to style"),
    };
    unlocked(project, layer)?;
    Ok(words)
}

/// The lettering category of `layer` in `project` as `lettering` reads it:
/// a block's, none for a text layer, which has none.
fn category(
    project: &Project,
    layer: LayerId,
    lettering: &LetteringSettings,
) -> Result<Option<LetteringCategory>> {
    let (page, _) = project.parent(layer)?;
    Ok(lettering::effective(page, layer, lettering).map(|effective| effective.category))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use anyhow::Context as _;
    use booharu_core::{
        attrs::{self, Lettering, LetteringSource},
        emphasis::EmphasisStyle,
    };
    use koharu_scene::{Created, Ref, Rgba, RichText, Session, Span};
    use serde_json::{Value, json};
    use tempfile::TempDir;

    use super::*;
    use crate::boxes::Refusal;

    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
    const BLUE: Rgba = Rgba([0.0, 0.0, 1.0, 1.0]);
    const BLACK: Rgba = Rgba([0.0, 0.0, 0.0, 1.0]);

    /// The translation the blocks hold: 魔 and 王 are three bytes each, so
    /// "魔王" is 4..10 and "NO" 11..13.
    const TEXT: &str = "Why 魔王 NO?";

    /// A page holding the translated blocks `plain`, lettered at size 30,
    /// `heavy`, bold italic, and `shout`, labelled a shout by hand, the
    /// untranslated block `bare`, the text layer `typed`, the raster
    /// `paint`, and the block `held` in the locked group `locked`.
    struct Fixture {
        _dir: TempDir,
        session: Session,
        layers: BTreeMap<String, LayerId>,
    }

    impl Fixture {
        fn new() -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let block = |x: f64, chars: Value| {
                json!({"kind": "block", "source": {
                    "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 120.0}}},
                    "content": "どうして",
                    "style": {"chars": {"size": 30.0, "fill": BLUE}},
                }, "translation": {
                    "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 120.0}}},
                    "content": TEXT,
                    "style": {"chars": chars},
                }, "attributes": {"c2pa": {"detection": {"action": "c2pa.created"}}}})
            };
            let bare = json!({"kind": "block", "source": {
                "frame": {"box": {"bounds": {"x": 210.0, "y": 20.0, "width": 40.0, "height": 120.0}}},
                "content": "どうして",
                "style": {"chars": {"size": 32.0, "fill": BLUE}},
            }});
            let ops = [
                json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}),
                json!({"op": "add_layer", "id": "$plain", "page": "$p", "index": 0,
                    "layer": block(10.0, json!({"size": 30.0, "fill": BLUE}))}),
                json!({"op": "add_layer", "id": "$heavy", "page": "$p", "index": 1,
                    "layer": block(60.0, json!({"size": 30.0, "weight": 700, "italic": true}))}),
                json!({"op": "add_layer", "id": "$shout", "page": "$p", "index": 2,
                    "layer": block(110.0, json!({"size": 30.0}))}),
                json!({"op": "add_layer", "id": "$bare", "page": "$p", "index": 3, "layer": bare}),
                json!({"op": "add_layer", "id": "$typed", "page": "$p", "index": 4,
                    "layer": {"kind": "text", "content": TEXT, "style": {"chars": {"size": 20.0}}}}),
                json!({"op": "add_layer", "id": "$paint", "page": "$p", "index": 5,
                    "layer": {"kind": "raster"}}),
                json!({"op": "add_layer", "id": "$locked", "page": "$p", "index": 6,
                    "layer": {"kind": "group"}}),
                json!({"op": "add_layer", "id": "$held", "page": "$p", "parent": "$locked",
                    "index": 0, "layer": block(160.0, json!({"size": 30.0}))}),
            ];
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?,
            })?;
            let layers: BTreeMap<String, LayerId> = change
                .created
                .into_iter()
                .filter_map(|created| match created {
                    Created::Layer {
                        id,
                        label: Some(label),
                    } => Some((label, id)),
                    _ => None,
                })
                .collect();
            let label = Lettering {
                category: LetteringCategory::Shout,
                source: LetteringSource::User,
                confidence: None,
                role: None,
            };
            session.apply(Edit {
                label: "test".to_owned(),
                ops: vec![
                    attrs::set(layers["shout"], &label)?,
                    Op::UpdateLayer {
                        layer: layers["locked"].into(),
                        patch: json!({"locked": true}),
                    },
                ],
            })?;
            Ok(Self {
                _dir: dir,
                session,
                layers,
            })
        }

        fn id(&self, name: &str) -> LayerId {
            self.layers[name]
        }

        /// The edit styling `range` of `name`'s words as `change` says, with
        /// the settings [`emphasis`] gives and no category fonts.
        fn style(
            &self,
            name: &str,
            range: Range<usize>,
            change: &WordChange,
        ) -> Result<Option<Edit>> {
            style_words(
                &self.session.project(),
                self.id(name),
                range,
                None,
                change,
                &emphasis(),
                &LetteringSettings::default(),
            )
        }

        /// Styles `range` of `name`'s words as `change` says, as one undo
        /// step holding one `SetRichText`, and answers the words.
        fn styled(
            &mut self,
            name: &str,
            range: Range<usize>,
            change: &WordChange,
        ) -> Result<RichText> {
            let edit = self.style(name, range, change)?.context("a change")?;
            assert_eq!(edit.label, LABEL);
            assert!(
                matches!(edit.ops.as_slice(), [Op::SetRichText { .. }]),
                "{:?}",
                edit.ops
            );
            let steps = self.session.history().steps.len();
            self.session.apply(edit)?;
            assert_eq!(self.session.history().steps.len(), steps + 1);
            self.words(name)
        }

        fn words(&self, name: &str) -> Result<RichText> {
            let project = self.session.project();
            Ok(match &project.layer(self.id(name))?.content {
                Content::Block(block) => block
                    .translation
                    .as_ref()
                    .context("a translation")?
                    .content
                    .clone(),
                Content::Text(text) => text.content.clone(),
                _ => bail!("{name} has no words"),
            })
        }
    }

    /// The default emphasis, with a shout's stressed words red.
    fn emphasis() -> EmphasisSettings {
        let mut settings = EmphasisSettings::default();
        settings.categories.insert(
            LetteringCategory::Shout,
            EmphasisStyle {
                fill: Some(RED),
                ..EmphasisStyle::default()
            },
        );
        settings
    }

    fn span(insert: &str, attributes: CharStyle) -> Span {
        Span {
            insert: insert.to_owned(),
            attributes,
        }
    }

    fn bold(on: bool) -> WordChange {
        WordChange {
            bold: Some(on),
            ..WordChange::default()
        }
    }

    fn italic(on: bool) -> WordChange {
        WordChange {
            italic: Some(on),
            ..WordChange::default()
        }
    }

    /// `TEXT` with "NO" marked `marks`.
    fn no_marked(marks: CharStyle) -> RichText {
        RichText(vec![
            span("Why 魔王 ", CharStyle::default()),
            span("NO", marks),
            span("?", CharStyle::default()),
        ])
    }

    fn weight(weight: u16) -> CharStyle {
        CharStyle {
            weight: Some(weight),
            ..CharStyle::default()
        }
    }

    #[test]
    fn a_range_off_a_character_or_past_the_text_is_refused() -> Result<()> {
        let fixture = Fixture::new()?;
        let backwards = Range { start: 10, end: 4 };
        for range in [5..10, 4..8, 11..15, 20..22, 4..4, backwards] {
            let refused = fixture.style("plain", range.clone(), &bold(true));
            assert!(refused.is_err(), "{range:?}");
        }
        // The whole text, and one character of three bytes, are taken.
        assert!(
            fixture
                .style("plain", 0..TEXT.len(), &bold(true))?
                .is_some()
        );
        assert!(fixture.style("plain", 4..7, &bold(true))?.is_some());
        Ok(())
    }

    #[test]
    fn only_emphasis_reads_the_settings() -> Result<()> {
        // A section that can't be read, as a bad one edited into the file.
        let unreadable = || -> Result<(EmphasisSettings, LetteringSettings)> {
            bail!("Booharu's emphasis can't be read from [booharu-emphasis]")
        };
        assert!(settings(true, unreadable).is_err());
        // B, I, Clear and the type bar's values go through without them.
        let (emphasis, lettering) = settings(false, unreadable)?;
        let mut fixture = Fixture::new()?;
        let edit = style_words(
            &fixture.session.project(),
            fixture.id("plain"),
            11..13,
            None,
            &bold(true),
            &emphasis,
            &lettering,
        )?
        .context("a change")?;
        fixture.session.apply(edit)?;
        assert_eq!(fixture.words("plain")?, no_marked(weight(700)));
        Ok(())
    }

    #[test]
    fn bold_and_italic_off_take_the_mark_off_or_set_the_regular() -> Result<()> {
        let mut fixture = Fixture::new()?;
        // In a regular text, bold marks the words and bold off takes the
        // mark off again.
        assert_eq!(
            fixture.styled("plain", 11..13, &bold(true))?,
            no_marked(weight(700))
        );
        assert_eq!(
            fixture.styled("plain", 11..13, &bold(false))?,
            RichText::from(TEXT)
        );
        assert!(fixture.style("plain", 11..13, &bold(false))?.is_none());
        assert_eq!(
            fixture.styled("plain", 11..13, &italic(true))?,
            no_marked(CharStyle {
                italic: Some(true),
                ..CharStyle::default()
            })
        );
        assert_eq!(
            fixture.styled("plain", 11..13, &italic(false))?,
            RichText::from(TEXT)
        );

        // In a bold italic text, off sets regular and upright.
        assert_eq!(
            fixture.styled("heavy", 11..13, &bold(false))?,
            no_marked(weight(400))
        );
        assert_eq!(
            fixture.styled("heavy", 11..13, &italic(false))?,
            no_marked(CharStyle {
                weight: Some(400),
                italic: Some(false),
                ..CharStyle::default()
            })
        );
        // And on again, its own values as marks.
        assert_eq!(
            fixture.styled("heavy", 11..13, &bold(true))?,
            no_marked(CharStyle {
                weight: Some(700),
                italic: Some(false),
                ..CharStyle::default()
            })
        );
        Ok(())
    }

    #[test]
    fn clear_takes_off_only_the_marks_in_the_range() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let red = WordChange {
            set: CharStyle {
                fill: Some(RED),
                size: Some(40.0),
                ..CharStyle::default()
            },
            ..WordChange::default()
        };
        fixture.styled("plain", 0..3, &red)?;
        fixture.styled("plain", 11..13, &bold(true))?;
        let clear = WordChange {
            clear: true,
            ..WordChange::default()
        };
        // From the middle of "NO" to the end: "N" keeps its mark.
        assert_eq!(
            fixture.styled("plain", 12..14, &clear)?,
            RichText(vec![
                span("Why", red.set.clone()),
                span(" 魔王 ", CharStyle::default()),
                span("N", weight(700)),
                span("O?", CharStyle::default()),
            ])
        );
        assert!(fixture.style("plain", 12..14, &clear)?.is_none());
        Ok(())
    }

    #[test]
    fn emphasis_follows_the_layers_category_or_the_default() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let stress = WordChange {
            emphasis: true,
            ..WordChange::default()
        };
        // A shout's: bold, at 115% of its size, and red.
        let shout = CharStyle {
            weight: Some(700),
            size: Some(34.5),
            fill: Some(RED),
            ..CharStyle::default()
        };
        assert_eq!(fixture.styled("shout", 11..13, &stress)?, no_marked(shout));
        // A block whose category has no emphasis of its own takes the
        // default, which keeps the text's fill.
        let default = CharStyle {
            weight: Some(700),
            size: Some(34.5),
            ..CharStyle::default()
        };
        assert_eq!(
            fixture.styled("plain", 11..13, &stress)?,
            no_marked(default)
        );
        // A text layer has no category: the default, over its own size.
        assert_eq!(
            fixture.styled("typed", 11..13, &stress)?,
            no_marked(CharStyle {
                weight: Some(700),
                size: Some(23.0),
                ..CharStyle::default()
            })
        );
        // A bold italic text's stress needs no weight.
        assert_eq!(
            fixture.styled("heavy", 11..13, &stress)?,
            no_marked(CharStyle {
                size: Some(34.5),
                ..CharStyle::default()
            })
        );
        // Stressed again, nothing changes.
        assert!(fixture.style("heavy", 11..13, &stress)?.is_none());
        Ok(())
    }

    #[test]
    fn the_second_border_is_set_and_cleared_on_a_word() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let border = WordChange {
            set: CharStyle {
                stroke: Some(RED),
                stroke_width: Some(2.0),
                outer_stroke: Some(BLACK),
                outer_stroke_width: Some(4.0),
                ..CharStyle::default()
            },
            ..WordChange::default()
        };
        assert_eq!(
            fixture.styled("plain", 4..10, &border)?,
            RichText(vec![
                span("Why ", CharStyle::default()),
                span("魔王", border.set.clone()),
                span(" NO?", CharStyle::default()),
            ])
        );
        // Set again, nothing changes; cleared, it goes.
        assert!(fixture.style("plain", 4..10, &border)?.is_none());
        let clear = WordChange {
            clear: true,
            ..WordChange::default()
        };
        assert_eq!(
            fixture.styled("plain", 4..10, &clear)?,
            RichText::from(TEXT)
        );
        Ok(())
    }

    #[test]
    fn a_text_changed_since_the_words_were_chosen_is_refused() -> Result<()> {
        let fixture = Fixture::new()?;
        let project = fixture.session.project();
        let style = |expected: &str| {
            style_words(
                &project,
                fixture.id("plain"),
                11..13,
                Some(expected),
                &bold(true),
                &emphasis(),
                &LetteringSettings::default(),
            )
        };
        let changed = style("Why 魔王 NO!").expect_err("a changed text");
        assert_eq!(
            changed.to_string(),
            "The text changed. Select the words again."
        );
        assert!(style(TEXT)?.is_some());
        Ok(())
    }

    #[test]
    fn a_change_asks_for_one_thing_on_a_text_that_takes_it() -> Result<()> {
        let fixture = Fixture::new()?;
        let choose = "Choose clear, emphasis, step, or explicit style fields.";
        let red = CharStyle {
            fill: Some(RED),
            ..CharStyle::default()
        };
        let bigger = Some(SizeStep::Bigger);
        for change in [
            WordChange::default(),
            WordChange {
                clear: true,
                emphasis: true,
                ..WordChange::default()
            },
            WordChange {
                step: bigger,
                clear: true,
                ..WordChange::default()
            },
            WordChange {
                step: bigger,
                emphasis: true,
                ..WordChange::default()
            },
            WordChange {
                step: bigger,
                bold: Some(true),
                ..WordChange::default()
            },
            WordChange {
                step: Some(SizeStep::Smaller),
                set: red.clone(),
                ..WordChange::default()
            },
            WordChange {
                emphasis: true,
                bold: Some(true),
                ..WordChange::default()
            },
            WordChange {
                clear: true,
                set: red.clone(),
                ..WordChange::default()
            },
        ] {
            let refused = fixture
                .style("plain", 11..13, &change)
                .expect_err("refused");
            assert_eq!(refused.to_string(), choose, "{change:?}");
        }
        let twice = WordChange {
            set: weight(900),
            bold: Some(true),
            ..WordChange::default()
        };
        assert!(fixture.style("plain", 11..13, &twice).is_err());
        // Explicit values go together.
        let together = WordChange {
            set: red,
            bold: Some(true),
            italic: Some(true),
            ..WordChange::default()
        };
        assert!(fixture.style("plain", 11..13, &together)?.is_some());

        // Neither a block nor a text, a block with no translation, and a
        // locked one, each refused.
        assert!(fixture.style("paint", 0..1, &bold(true)).is_err());
        assert!(fixture.style("locked", 0..1, &bold(true)).is_err());
        assert!(fixture.style("bare", 0..1, &bold(true)).is_err());
        let locked = fixture
            .style("held", 11..13, &bold(true))
            .expect_err("a locked group");
        assert!(
            matches!(
                locked.downcast_ref::<Refusal>(),
                Some(Refusal::Locked { layer, .. }) if *layer == fixture.id("locked")
            ),
            "{locked}"
        );
        Ok(())
    }

    fn step(bigger: bool) -> WordChange {
        WordChange {
            step: Some(match bigger {
                true => SizeStep::Bigger,
                false => SizeStep::Smaller,
            }),
            ..WordChange::default()
        }
    }

    fn sized(size: f64) -> CharStyle {
        CharStyle {
            size: Some(size),
            ..CharStyle::default()
        }
    }

    #[test]
    fn a_step_steps_each_word_from_its_own_size() -> Result<()> {
        let mut fixture = Fixture::new()?;
        // "Why" at 40 in a text of 30.
        fixture.styled(
            "plain",
            0..3,
            &WordChange {
                set: sized(40.0),
                ..WordChange::default()
            },
        )?;
        let before = fixture.words("plain")?;
        assert_eq!(
            fixture.styled("plain", 0..TEXT.len(), &step(true))?,
            RichText(vec![
                span("Why", sized(42.0)),
                span(" 魔王 NO?", sized(31.5))
            ])
        );
        // A step back gives the words their sizes again, the text's own
        // as no mark.
        assert_eq!(
            fixture.styled("plain", 0..TEXT.len(), &step(false))?,
            before
        );
        // Only the range steps, its other marks kept: a bold "NO" in a text
        // layer of 20.
        fixture.styled("typed", 11..13, &bold(true))?;
        assert_eq!(
            fixture.styled("typed", 11..13, &step(false))?,
            no_marked(CharStyle {
                weight: Some(700),
                size: Some(19.0),
                ..CharStyle::default()
            })
        );
        Ok(())
    }

    #[test]
    fn a_step_past_the_largest_size_changes_nothing() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.styled(
            "plain",
            11..13,
            &WordChange {
                set: sized(300.0),
                ..WordChange::default()
            },
        )?;
        assert!(fixture.style("plain", 11..13, &step(true))?.is_none());
        assert!(fixture.style("plain", 11..13, &step(false))?.is_some());
        Ok(())
    }

    /// Marks `name`'s "NO" bold straight through the scene, unlocking the
    /// group `locked` around it for a moment, as a box styled before it was
    /// locked.
    fn mark(fixture: &mut Fixture, name: &str) -> Result<()> {
        let lock = |locked: bool| Edit {
            label: "test".to_owned(),
            ops: vec![Op::UpdateLayer {
                layer: fixture.id("locked").into(),
                patch: json!({"locked": locked}),
            }],
        };
        let (unlock, relock) = (lock(false), lock(true));
        let mark = Edit {
            label: "test".to_owned(),
            ops: vec![Op::SetRichText {
                layer: fixture.id(name).into(),
                spans: no_marked(weight(700)).0,
            }],
        };
        for edit in [unlock, mark, relock] {
            fixture.session.apply(edit)?;
        }
        Ok(())
    }

    fn clear(fixture: &Fixture, names: &[&str]) -> Result<ClearedWords> {
        let layers: Vec<LayerId> = names.iter().map(|name| fixture.id(name)).collect();
        clear_all(&fixture.session.project(), &layers)
    }

    /// The layers whose words an edit sets, each by one `SetRichText`.
    fn set_words(edit: &Edit) -> Vec<Ref> {
        edit.ops
            .iter()
            .map(|op| match op {
                Op::SetRichText { layer, .. } => layer.clone(),
                op => panic!("only words set: {op:?}"),
            })
            .collect()
    }

    fn refs(fixture: &Fixture, names: &[&str]) -> Vec<Ref> {
        names.iter().map(|name| fixture.id(name).into()).collect()
    }

    #[test]
    fn clearing_two_boxes_is_one_undo_step() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.styled(
            "plain",
            0..3,
            &WordChange {
                set: sized(40.0),
                ..WordChange::default()
            },
        )?;
        fixture.styled("plain", 11..13, &bold(true))?;
        fixture.styled("typed", 4..10, &italic(true))?;
        let styled = (fixture.words("plain")?, fixture.words("typed")?);
        let cleared = clear(&fixture, &["plain", "typed", "plain"])?;
        assert_eq!((cleared.cleared, cleared.locked), (2, 0));
        let edit = cleared.edit.context("an edit")?;
        assert_eq!(edit.label, CLEAR_LABEL);
        assert_eq!(set_words(&edit), refs(&fixture, &["plain", "typed"]));
        let steps = fixture.session.history().steps.len();
        fixture.session.apply(edit)?;
        assert_eq!(fixture.session.history().steps.len(), steps + 1);
        assert_eq!(fixture.words("plain")?, RichText::from(TEXT));
        assert_eq!(fixture.words("typed")?, RichText::from(TEXT));
        // One undo gives both their word styles back.
        fixture.session.undo()?;
        assert_eq!((fixture.words("plain")?, fixture.words("typed")?), styled);
        Ok(())
    }

    #[test]
    fn a_locked_box_is_counted_and_left_as_it_is() -> Result<()> {
        let mut fixture = Fixture::new()?;
        mark(&mut fixture, "plain")?;
        mark(&mut fixture, "held")?;
        // The locked group itself is no text, so it counts in neither.
        let cleared = clear(&fixture, &["plain", "held", "locked"])?;
        assert_eq!((cleared.cleared, cleared.locked), (1, 1));
        let edit = cleared.edit.context("an edit")?;
        assert_eq!(set_words(&edit), refs(&fixture, &["plain"]));
        fixture.session.apply(edit)?;
        assert_eq!(fixture.words("plain")?, RichText::from(TEXT));
        assert_eq!(fixture.words("held")?, no_marked(weight(700)));
        // Locked alone, there's nothing to clear.
        let cleared = clear(&fixture, &["held"])?;
        assert_eq!((cleared.cleared, cleared.locked), (0, 1));
        assert!(cleared.edit.is_none());
        Ok(())
    }

    #[test]
    fn a_box_without_word_styles_is_left_as_it_is() -> Result<()> {
        let mut fixture = Fixture::new()?;
        mark(&mut fixture, "plain")?;
        // "heavy" is bold italic as a whole, which is no word style.
        let cleared = clear(&fixture, &["heavy", "plain", "shout"])?;
        assert_eq!((cleared.cleared, cleared.locked), (1, 0));
        let edit = cleared.edit.context("an edit")?;
        assert_eq!(set_words(&edit), refs(&fixture, &["plain"]));
        Ok(())
    }

    #[test]
    fn a_block_without_a_translation_or_another_layer_is_ignored() -> Result<()> {
        let fixture = Fixture::new()?;
        let cleared = clear(&fixture, &["bare", "paint"])?;
        assert_eq!((cleared.cleared, cleared.locked), (0, 0));
        assert!(cleared.edit.is_none());
        Ok(())
    }

    #[test]
    fn nothing_to_clear_gives_no_edit() -> Result<()> {
        let fixture = Fixture::new()?;
        for names in [&["plain", "typed", "heavy"][..], &[]] {
            let cleared = clear(&fixture, names)?;
            assert_eq!((cleared.cleared, cleared.locked), (0, 0), "{names:?}");
            assert!(cleared.edit.is_none(), "{names:?}");
        }
        // The answer the desktop gives holds the counts alone.
        let answer = serde_json::to_value(clear(&fixture, &["plain"])?)?;
        assert_eq!(answer, json!({"cleared": 0, "locked": 0}));
        Ok(())
    }

    #[test]
    fn the_text_words_are_found_in_is_refused_as_its_layer_is() -> Result<()> {
        let fixture = Fixture::new()?;
        let project = fixture.session.project();
        assert_eq!(text(&project, fixture.id("plain"))?.content.text(), TEXT);
        assert_eq!(
            text(&project, fixture.id("typed"))?.style.chars.size,
            Some(20.0)
        );
        for refused in ["bare", "paint", "locked", "held"] {
            assert!(text(&project, fixture.id(refused)).is_err(), "{refused}");
        }
        Ok(())
    }

    #[test]
    fn the_star_gives_the_emphasis_over_the_texts_own_values() -> Result<()> {
        let fixture = Fixture::new()?;
        let project = fixture.session.project();
        let star = |name: &str| {
            emphasis_patch(
                &project,
                fixture.id(name),
                &emphasis(),
                &LetteringSettings::default(),
            )
        };
        assert_eq!(
            star("shout")?,
            CharStyle {
                weight: Some(700),
                size: Some(34.5),
                fill: Some(RED),
                ..CharStyle::default()
            }
        );
        assert_eq!(
            star("typed")?,
            CharStyle {
                weight: Some(700),
                size: Some(23.0),
                ..CharStyle::default()
            }
        );
        // A block not yet translated: over what Koharu's placement would
        // give its translation, at the page's lettering size, the lower
        // median of its sources' sizes, 30.
        assert_eq!(
            star("bare")?,
            CharStyle {
                weight: Some(700),
                size: Some(34.5),
                ..CharStyle::default()
            }
        );
        for refused in ["paint", "held"] {
            assert!(star(refused).is_err(), "{refused}");
        }
        Ok(())
    }
}
