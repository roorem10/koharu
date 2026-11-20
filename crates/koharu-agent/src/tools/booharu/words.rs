//! Styling words, as the agent does it: some characters of a block's
//! translation or of a text layer made bold or italic, stressed as their
//! lettering category's emphasis says, given values of their own, or
//! cleared, as one undo step worked out by `booharu::words`, as the app's
//! word toolbar and type bar style words chosen by hand. Koharu's `edit`
//! can't: its schema leaves out `set_rich_text`, the op that marks single
//! characters. The agent names the words by their wording, found as a
//! translation run finds the words its model picks to stress, or by Unicode
//! character offsets, which the tool turns into the UTF-8 byte offsets the
//! scene takes. The edit leaves the hand-edit record as it is, so a word it
//! styles is a hand edit, as one styled in the app is.

use std::{
    ops::{Range, RangeInclusive},
    slice,
};

use anyhow::{Context as _, ensure};
use booharu::words::{self, WordChange};
use booharu_core::{
    emphasis::{self, EmphasisSettings},
    settings::LetteringSettings,
};
use koharu_scene::{Change, CharStyle, LayerId, Rgba};
use rig_core::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::{Tools, failed, project, schema};

/// The sizes a word takes, as a percentage of its text's.
const SIZE_PERCENT: RangeInclusive<f64> = 25.0..=400.0;

/// The widest border a word takes, in pixels, as an emphasis look takes.
const MAX_WIDTH: f64 = 100.0;

/// The size the renderer draws a text with none at, in pixels.
const SIZE: f64 = 24.0;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct StyleWordsArgs {
    /// The block, whose translation's words are styled, or the text layer.
    layer: LayerId,
    /// The words, found where they first occur in the text as whole words,
    /// exact, case and all; anywhere for a word holding a Chinese, Japanese,
    /// or Korean character. Not with `start` and `end`.
    text: Option<String>,
    /// Where the words start: the first one's offset in the text, in
    /// Unicode characters from 0. With `end`, not with `text`.
    start: Option<usize>,
    /// Where the words end: the offset just after the last one, in Unicode
    /// characters. With `start`.
    end: Option<usize>,
    /// Stress the words in the look `booharu-emphasis` sets for the block's
    /// lettering category, a text layer's the default one. Not with values
    /// of the words' own or `clear`.
    #[serde(default)]
    emphasis: bool,
    /// Bold, weight 700; false gives the words the box's own weight back,
    /// regular in a bold box.
    bold: Option<bool>,
    /// Italic; false gives the words the box's own slant back, upright in an
    /// italic box.
    italic: Option<bool>,
    /// The font family, never blank.
    family: Option<String>,
    /// The size, as a percentage of the box's own, from 25 to 400.
    #[schemars(range(min = 25.0, max = 400.0))]
    size_percent: Option<f64>,
    /// The words' colour, as [red, green, blue, alpha] from 0 to 1.
    fill: Option<Rgba>,
    /// The outline's colour, as [red, green, blue, alpha] from 0 to 1.
    outline: Option<Rgba>,
    /// The outline's width in pixels, from 0 to 100; 0 draws none.
    #[schemars(range(min = 0.0, max = 100.0))]
    outline_width: Option<f64>,
    /// The second border's colour, drawn outside the outline and only with
    /// one, as [red, green, blue, alpha] from 0 to 1.
    outer_stroke: Option<Rgba>,
    /// The second border's width in pixels, from 0 to 100.
    #[schemars(range(min = 0.0, max = 100.0))]
    outer_stroke_width: Option<f64>,
    /// Take every value of the words' own off, so they look as the rest of
    /// the box. Not with `emphasis` or values of the words' own.
    #[serde(default)]
    clear: bool,
}

/// The words a call names.
enum Pick {
    /// By their wording.
    Text(String),
    /// By their Unicode character offsets, end exclusive.
    Chars(Range<usize>),
}

impl Pick {
    /// The words' UTF-8 byte offsets in `text`, as the scene takes them.
    fn bytes(&self, text: &str) -> anyhow::Result<Range<usize>> {
        match self {
            Self::Text(words) => emphasis::find_words(text, slice::from_ref(words))
                .into_iter()
                .next()
                .context("text was not found as a whole word or phrase"),
            Self::Chars(range) => {
                let length = text.chars().count();
                ensure!(
                    range.start < range.end && range.end <= length,
                    "start {} and end {} hold no characters of the text, which is {length} \
                     Unicode characters long: start comes before end, and end is at most \
                     {length}",
                    range.start,
                    range.end
                );
                let byte = |offset| {
                    text.char_indices()
                        .nth(offset)
                        .map_or(text.len(), |(at, _)| at)
                };
                Ok(byte(range.start)..byte(range.end))
            }
        }
    }
}

impl StyleWordsArgs {
    /// The words the call names: by `text`, or by both `start` and `end`.
    fn pick(&self) -> Result<Pick, ToolExecutionError> {
        match (&self.text, self.start, self.end) {
            (Some(text), None, None) => Ok(Pick::Text(text.clone())),
            (None, Some(start), Some(end)) => Ok(Pick::Chars(start..end)),
            _ => Err(ToolExecutionError::invalid_args(
                "Choose text, or both start and end (Unicode character offsets).",
            )),
        }
    }

    /// Refuses a value of the words' own out of bounds: a blank family, a
    /// size out of 25 to 400 percent, a colour channel out of 0 to 1, or a
    /// width out of 0 to 100 pixels.
    fn check(&self) -> Result<(), ToolExecutionError> {
        let refused = |problem: String| Err(ToolExecutionError::invalid_args(problem));
        if self
            .family
            .as_deref()
            .is_some_and(|family| family.trim().is_empty())
        {
            return refused("`family` is blank: name a font family, or leave it out".to_owned());
        }
        if let Some(percent) = self.size_percent
            && !SIZE_PERCENT.contains(&percent)
        {
            return refused(format!(
                "`size_percent` {percent} isn't from {} to {}",
                SIZE_PERCENT.start(),
                SIZE_PERCENT.end()
            ));
        }
        let colours = [
            ("fill", self.fill),
            ("outline", self.outline),
            ("outer_stroke", self.outer_stroke),
        ];
        for (name, colour) in colours {
            if let Some(Rgba(channels)) = colour
                && !channels.iter().all(|channel| (0.0..=1.0).contains(channel))
            {
                return refused(format!(
                    "`{name}` {channels:?} isn't four channels from 0 to 1"
                ));
            }
        }
        let widths = [
            ("outline_width", self.outline_width),
            ("outer_stroke_width", self.outer_stroke_width),
        ];
        for (name, width) in widths {
            if let Some(width) = width
                && !(0.0..=MAX_WIDTH).contains(&width)
            {
                return refused(format!(
                    "`{name}` {width} isn't from 0 to {MAX_WIDTH} pixels"
                ));
            }
        }
        Ok(())
    }

    /// What the words edit does to the words, `size_percent` taken of
    /// `size`, the text's own, or of the renderer's 24 pixels without one,
    /// to a hundredth of a pixel, as an emphasis's size is.
    fn change(&self, size: Option<f64>) -> WordChange {
        let size = self
            .size_percent
            .map(|percent| (size.unwrap_or(SIZE) * percent).round() / 100.0);
        WordChange {
            set: CharStyle {
                family: self.family.clone(),
                size,
                fill: self.fill,
                stroke: self.outline,
                stroke_width: self.outline_width,
                outer_stroke: self.outer_stroke,
                outer_stroke_width: self.outer_stroke_width,
                ..CharStyle::default()
            },
            bold: self.bold,
            italic: self.italic,
            emphasis: self.emphasis,
            clear: self.clear,
        }
    }
}

/// What styling the words did.
#[derive(Debug, Serialize)]
pub(super) struct Styled {
    /// The words styled, as the text holds them.
    words: String,
    /// Where they start in the text, in Unicode characters from 0.
    start: usize,
    /// Where they end, in Unicode characters, exclusive.
    end: usize,
    /// What changed; none when the words already looked so, which makes no
    /// undo step.
    change: Option<Change>,
}

pub(super) struct StyleWords;

impl Tool for StyleWords {
    const NAME: &'static str = "booharu_style_words";
    type Args = StyleWordsArgs;
    type Output = Styled;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Style some words of a block's translation or of a text layer, as one undo step, as the \
         app's word toolbar and type bar style words chosen by hand; Koharu's `edit` can't style \
         single words. Name the words by `text`, found where they first occur as whole words, \
         exact and case-sensitive (a word holding a Chinese, Japanese, or Korean character is \
         found anywhere, as those scripts don't space their words), or by `start` and `end`, \
         Unicode character offsets into the text, end exclusive, to reach a later occurrence or \
         part of a word. Then give exactly one of: `emphasis` true, the stressed look \
         `booharu-emphasis` sets for the block's lettering category (booharu_get_text_styles \
         tells a block's category), or the default one for a text layer, bold at 115% of the \
         box's size to start with; values of the words' own, which go together: `bold`, \
         `italic`, `family`, `size_percent` (25 to 400, of the box's own size), `fill`, \
         `outline` and `outline_width`, and a second border drawn outside the outline, and only \
         with one, `outer_stroke` and `outer_stroke_width`, colours as [red, green, blue, \
         alpha] from 0 to 1 and widths in pixels from 0 to 100; or `clear` true, which takes \
         every value of the words' own off, a translation run's emphasis included, so they look \
         as the rest of the box. `bold` false and `italic` false give the words the box's own \
         weight or slant back, so in a bold or italic box they set regular or upright. A word's \
         own values win over the box's: changing the box's look with `edit` leaves them, so \
         clear the words first when they should follow it. Words given values of their own, an \
         emphasis included, count as a hand edit of the block's look unless the only one is a \
         size, as words styled in the app do, so booharu_apply_text_style leaves the block alone \
         unless told to include hand-edited blocks, and booharu_set_lettering_category keeps its \
         font. It refuses a locked layer, or one inside a locked layer, and a block without a \
         translation. Answers the `words` styled, their `start` and `end` in characters, and what changed \
         (`change`), null when they already looked so, which makes no undo step. Example: \
         {\"layer\": \"<id>\", \"text\": \"NO\", \"emphasis\": true}."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<StyleWordsArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: StyleWordsArgs,
    ) -> Result<Styled, Self::Error> {
        let pick = args.pick()?;
        args.check()?;
        let tools = project(context)?;
        // On a blocking thread, as `edit` applies, and under one lock, so the
        // words found are the ones styled.
        tokio::task::spawn_blocking(move || -> anyhow::Result<Styled> {
            // Only emphasis reads the settings: the other changes set the
            // values the call names.
            let (emphasis, lettering) = words::settings(args.emphasis, || settings(&tools))?;
            let mut session = tools.session.lock();
            let project = session.project();
            let text = words::text(&project, args.layer)?;
            let plain = text.content.text();
            let range = pick.bytes(&plain)?;
            let asked = args.change(text.style.chars.size);
            let edit = words::style_words(
                &project,
                args.layer,
                range.clone(),
                None,
                &asked,
                &emphasis,
                &lettering,
            )?;
            let change = edit.map(|edit| session.apply(edit)).transpose()?;
            Ok(Styled {
                start: plain[..range.start].chars().count(),
                end: plain[..range.end].chars().count(),
                words: plain[range].to_owned(),
                change,
            })
        })
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
    }
}

/// `[booharu-emphasis]` and `[booharu-lettering]` as the app holds them now,
/// or outside the app as the file holds them: how a stressed word looks, and
/// the lettering categories that choose its look. A section that can't be
/// read is an error, so no word is stressed in a look nobody chose.
fn settings(tools: &Tools) -> anyhow::Result<(EmphasisSettings, LetteringSettings)> {
    Ok(match &tools.booharu {
        Some(host) => {
            let preferences = host.preferences()?;
            (preferences.emphasis, preferences.lettering)
        }
        None => (EmphasisSettings::read()?, koharu_config::get()?),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use booharu_core::{
        edited, emphasis::EmphasisStyle, lettering::LetteringCategory, settings::BooharuPreferences,
    };
    use koharu_pipeline::{Pipeline, PipelineConfig};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::{Content, Created, Edit, Layer, RichText, Session, Span};
    use parking_lot::Mutex;
    use rig_core::tool::ToolErrorKind;
    use serde_json::{Map, json};

    use super::*;
    use crate::tools::booharu::BooharuHost;

    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
    const WHITE: Rgba = Rgba([1.0; 4]);
    const BLACK: Rgba = Rgba([0.0, 0.0, 0.0, 1.0]);

    /// The translation most blocks hold. The emoji is one character of four
    /// bytes: "NO" is characters 2..4, bytes 5..7, "KNOW" characters 6..10,
    /// and the last "NO" characters 11..13, bytes 14..16, of 14 characters.
    const LINE: &str = "🙂 NO! KNOW NO.";

    /// The app, whose shouts' stressed words are red over the default
    /// emphasis Booharu starts with, or, `broken`, one whose settings can't
    /// be read.
    struct App {
        broken: bool,
    }

    impl BooharuHost for App {
        fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
            anyhow::ensure!(
                !self.broken,
                "[booharu-reading] pages_per_request = 99 is over 40"
            );
            let mut preferences = BooharuPreferences::default();
            preferences.emphasis.categories.insert(
                LetteringCategory::Shout,
                EmphasisStyle {
                    fill: Some(RED),
                    ..EmphasisStyle::default()
                },
            );
            Ok(preferences)
        }

        fn save_section(
            &self,
            _: &str,
            _: Map<String, Value>,
        ) -> anyhow::Result<BooharuPreferences> {
            anyhow::bail!("nothing here saves settings")
        }

        fn font_families(&self) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }
    }

    /// A page holding blocks found, read, and translated by Koharu's stages,
    /// each lettered at size 30: `line`, `LINE`, `shout`, "NO WAY!" labelled
    /// a shout by hand, `cjk`, "魔王だ魔王", and `locked`, `LINE` and
    /// locked; the untranslated block `bare`, the text layer `note`, "Hey
    /// NO" at size 20, and the raster `paint`. The tools on it, in `app`,
    /// and the layers' ids.
    fn open(app: App) -> (tempfile::TempDir, Tools, impl Fn(&str) -> LayerId) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let c2pa = json!({
            "detection": {"action": "c2pa.created"},
            "ocr": {"action": "c2pa.created"},
            "translation": {"action": "c2pa.translated"},
        });
        let block = |label: &str, translation: Option<&str>, locked: bool, attributes: Value| {
            let mut attributes = attributes;
            attributes["c2pa"] = c2pa.clone();
            let mut layer = json!({"kind": "block", "locked": locked,
                "source": {"content": "やあ"}, "attributes": attributes});
            if let Some(translation) = translation {
                layer["translation"] =
                    json!({"content": translation, "style": {"chars": {"size": 30.0}}});
            }
            json!({"op": "add_layer", "id": format!("${label}"), "page": "$page", "index": 0, "layer": layer})
        };
        let shout = json!({"booharu": {"lettering": {"category": "shout", "source": "user"}}});
        let ops = [
            json!({"op": "add_page", "id": "$page", "index": 0, "size": {"width": 400, "height": 400}}),
            block("line", Some(LINE), false, json!({})),
            block("shout", Some("NO WAY!"), false, shout),
            block("cjk", Some("魔王だ魔王"), false, json!({})),
            block("locked", Some(LINE), true, json!({})),
            block("bare", None, false, json!({})),
            json!({"op": "add_layer", "id": "$note", "page": "$page", "index": 0,
                "layer": {"kind": "text", "content": "Hey NO", "style": {"chars": {"size": 20.0}}}}),
            json!({"op": "add_layer", "id": "$paint", "page": "$page", "index": 0,
                "layer": {"kind": "raster"}}),
        ];
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(|op| serde_json::from_value(op).expect("an op"))
                    .collect(),
            })
            .expect("a page");
        let ids: Vec<(String, LayerId)> = change
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
        let id = move |label: &str| {
            ids.iter()
                .find_map(|(found, id)| (found == label).then_some(*id))
                .expect("a layer")
        };
        let tools = Tools::new(
            Arc::new(Mutex::new(session)),
            Pipeline::new(PipelineConfig::default()),
            Renderer::new(RendererConfig::default()),
        )
        .with_booharu(Arc::new(app));
        (dir, tools, id)
    }

    /// The tools on [`open`]'s page in an app whose settings read.
    fn opened() -> (tempfile::TempDir, Tools, impl Fn(&str) -> LayerId) {
        open(App { broken: false })
    }

    /// Calls the tool with `args`, as the agent sends them.
    async fn style(tools: &Tools, args: Value) -> Result<Styled, ToolExecutionError> {
        let args = serde_json::from_value(args).expect("arguments the tool takes");
        StyleWords.call(&mut tools.tool_context(), args).await
    }

    /// The words `layer` holds: a block's translation's, or a text layer's.
    fn held(tools: &Tools, layer: LayerId) -> RichText {
        let project = tools.session.lock().project();
        match &project.layer(layer).expect("the layer").content {
            Content::Block(block) => block
                .translation
                .as_ref()
                .expect("a translation")
                .content
                .clone(),
            Content::Text(text) => text.content.clone(),
            _ => panic!("{layer} holds no words"),
        }
    }

    fn layer(tools: &Tools, layer: LayerId) -> Layer {
        let project = tools.session.lock().project();
        project.layer(layer).expect("the layer").clone()
    }

    /// The undo steps' labels.
    fn steps(tools: &Tools) -> Vec<String> {
        tools.session.lock().history().steps
    }

    fn span(insert: &str, attributes: CharStyle) -> Span {
        Span {
            insert: insert.to_owned(),
            attributes,
        }
    }

    fn plain(insert: &str) -> Span {
        span(insert, CharStyle::default())
    }

    /// Where the words styled are, as the call answers it.
    fn found(styled: &Styled) -> (&str, usize, usize) {
        (styled.words.as_str(), styled.start, styled.end)
    }

    #[tokio::test]
    async fn words_are_found_by_their_wording_or_by_character_offsets() {
        let (_dir, tools, id) = opened();
        let line = id("line");
        let before = steps(&tools).len();
        // By their wording: the first whole "NO", never the one in "KNOW",
        // at 130% of the box's size.
        let styled = style(
            &tools,
            json!({"layer": line, "text": "NO", "size_percent": 130}),
        )
        .await
        .expect("a larger word");
        assert_eq!(found(&styled), ("NO", 2, 4));
        assert!(styled.change.is_some());
        // By offsets, past the emoji's four bytes: the last "NO", bold.
        let styled = style(
            &tools,
            json!({"layer": line, "start": 11, "end": 13, "bold": true}),
        )
        .await
        .expect("a bold word");
        assert_eq!(found(&styled), ("NO", 11, 13));
        let bold = CharStyle {
            weight: Some(700),
            ..CharStyle::default()
        };
        let larger = CharStyle {
            size: Some(39.0),
            ..CharStyle::default()
        };
        assert_eq!(
            held(&tools, line),
            RichText(vec![
                plain("🙂 "),
                span("NO", larger.clone()),
                plain("! KNOW "),
                span("NO", bold),
                plain("."),
            ])
        );
        // One undo step each, under the words edit's label.
        let labels = steps(&tools);
        assert_eq!(labels.len(), before + 2);
        assert_eq!(labels.last().map(String::as_str), Some(words::LABEL));

        // Bold off in a regular box takes the weight off again; once off,
        // nothing changes, and no undo step is made.
        let regular = json!({"layer": line, "start": 11, "end": 13, "bold": false});
        let styled = style(&tools, regular.clone()).await.expect("regular");
        assert!(styled.change.is_some());
        let styled = style(&tools, regular).await.expect("regular again");
        assert!(styled.change.is_none());
        assert_eq!(steps(&tools).len(), before + 3);
        assert_eq!(
            held(&tools, line),
            RichText(vec![plain("🙂 "), span("NO", larger), plain("! KNOW NO."),])
        );
    }

    #[tokio::test]
    async fn emphasis_follows_the_lettering_category_and_clear_takes_every_value_off() {
        let (_dir, tools, id) = opened();
        let stressed = |size, fill| CharStyle {
            weight: Some(700),
            size: Some(size),
            fill,
            ..CharStyle::default()
        };
        // A shout's: red, over the default's bold at 115% of its size.
        let shout = id("shout");
        let stress = json!({"layer": shout, "text": "NO", "emphasis": true});
        style(&tools, stress).await.expect("a stressed shout");
        assert_eq!(
            held(&tools, shout),
            RichText(vec![span("NO", stressed(34.5, Some(RED))), plain(" WAY!")])
        );
        // A block whose category has none of its own takes the default.
        let line = id("line");
        let stress = json!({"layer": line, "start": 2, "end": 4, "emphasis": true});
        style(&tools, stress).await.expect("a stressed word");
        assert_eq!(
            held(&tools, line),
            RichText(vec![
                plain("🙂 "),
                span("NO", stressed(34.5, None)),
                plain("! KNOW NO."),
            ])
        );
        // A text layer has no category: the default, over its own size.
        let note = id("note");
        let stress = json!({"layer": note, "text": "NO", "emphasis": true});
        style(&tools, stress).await.expect("a stressed note");
        assert_eq!(
            held(&tools, note),
            RichText(vec![plain("Hey "), span("NO", stressed(23.0, None))])
        );

        // Clear takes every value of the words' own off; cleared again,
        // nothing changes, with no undo step.
        let clear = json!({"layer": shout, "start": 0, "end": 7, "clear": true});
        let styled = style(&tools, clear.clone()).await.expect("cleared");
        assert_eq!(found(&styled), ("NO WAY!", 0, 7));
        assert_eq!(held(&tools, shout), RichText::from("NO WAY!"));
        let before = steps(&tools).len();
        let styled = style(&tools, clear).await.expect("cleared again");
        assert!(styled.change.is_none());
        assert_eq!(steps(&tools).len(), before);
    }

    #[tokio::test]
    async fn the_second_border_is_set_and_cleared_on_a_word() {
        let (_dir, tools, id) = opened();
        let line = id("line");
        let border = json!({
            "layer": line,
            "text": "KNOW",
            "outline": WHITE,
            "outline_width": 2.0,
            "outer_stroke": BLACK,
            "outer_stroke_width": 4.0,
        });
        let styled = style(&tools, border.clone())
            .await
            .expect("a second border");
        assert_eq!(found(&styled), ("KNOW", 6, 10));
        let bordered = CharStyle {
            stroke: Some(WHITE),
            stroke_width: Some(2.0),
            outer_stroke: Some(BLACK),
            outer_stroke_width: Some(4.0),
            ..CharStyle::default()
        };
        assert_eq!(
            held(&tools, line),
            RichText(vec![
                plain("🙂 NO! "),
                span("KNOW", bordered),
                plain(" NO."),
            ])
        );
        // Set again, nothing changes; cleared, the word looks as the box.
        let styled = style(&tools, border).await.expect("the same border");
        assert!(styled.change.is_none());
        let clear = json!({"layer": line, "start": 6, "end": 10, "clear": true});
        style(&tools, clear).await.expect("cleared");
        assert_eq!(held(&tools, line), RichText::from(LINE));
    }

    #[tokio::test]
    async fn a_cjk_word_is_found_inside_unspaced_text() {
        let (_dir, tools, id) = opened();
        let cjk = id("cjk");
        // "王だ" isn't spaced from its neighbours, as no CJK word is, and
        // its offsets are characters, three bytes each.
        let italic = CharStyle {
            italic: Some(true),
            ..CharStyle::default()
        };
        let styled = style(
            &tools,
            json!({"layer": cjk, "text": "王だ", "italic": true}),
        )
        .await
        .expect("an italic word");
        assert_eq!(found(&styled), ("王だ", 1, 3));
        assert_eq!(
            held(&tools, cjk),
            RichText(vec![plain("魔"), span("王だ", italic), plain("魔王")])
        );
        // The second "魔王", by its offsets.
        let red = json!({"layer": cjk, "start": 3, "end": 5, "fill": RED});
        let styled = style(&tools, red).await.expect("a red word");
        assert_eq!(found(&styled), ("魔王", 3, 5));
        let words = held(&tools, cjk);
        assert_eq!(
            words.0.last(),
            Some(&span(
                "魔王",
                CharStyle {
                    fill: Some(RED),
                    ..CharStyle::default()
                }
            ))
        );
    }

    #[tokio::test]
    async fn a_call_naming_its_words_or_values_wrongly_changes_nothing() {
        let (_dir, tools, id) = opened();
        let line = id("line");
        let before = steps(&tools).len();
        // The arguments alone are refused.
        for args in [
            json!({"layer": line, "bold": true}),
            json!({"layer": line, "text": "NO", "start": 2, "end": 4, "bold": true}),
            json!({"layer": line, "start": 2, "bold": true}),
            json!({"layer": line, "text": "NO", "size_percent": 24}),
            json!({"layer": line, "text": "NO", "size_percent": 401}),
            json!({"layer": line, "text": "NO", "fill": [1.5, 0, 0, 1]}),
            json!({"layer": line, "text": "NO", "outer_stroke_width": 101}),
            json!({"layer": line, "text": "NO", "family": "  "}),
        ] {
            let error = style(&tools, args.clone()).await.expect_err("refused");
            assert_eq!(error.kind(), ToolErrorKind::InvalidArgs, "{args}: {error}");
        }
        // And so are words the text doesn't hold, a change asking for
        // none or more than one thing, and a layer that takes none.
        let choose = "Choose clear, emphasis, or explicit style fields.";
        for (args, reason) in [
            (
                json!({"layer": line, "text": "missing", "bold": true}),
                "text was not found as a whole word or phrase",
            ),
            (
                json!({"layer": line, "text": "KNO", "bold": true}),
                "text was not found as a whole word or phrase",
            ),
            (
                json!({"layer": line, "start": 13, "end": 15, "bold": true}),
                "which is 14 Unicode characters long",
            ),
            (
                json!({"layer": line, "start": 4, "end": 4, "bold": true}),
                "start 4 and end 4 hold no characters",
            ),
            (json!({"layer": line, "text": "NO"}), choose),
            (
                json!({"layer": line, "text": "NO", "emphasis": true, "bold": true}),
                choose,
            ),
            (
                json!({"layer": line, "text": "NO", "clear": true, "bold": false}),
                choose,
            ),
            (
                json!({"layer": id("locked"), "text": "NO", "bold": true}),
                " is locked",
            ),
            (
                json!({"layer": id("bare"), "start": 0, "end": 1, "bold": true}),
                "has no translation",
            ),
            (
                json!({"layer": id("paint"), "start": 0, "end": 1, "bold": true}),
                "is neither a block nor a text",
            ),
        ] {
            let error = style(&tools, args.clone()).await.expect_err("refused");
            assert!(error.to_string().contains(reason), "{args}: {error}");
        }
        assert_eq!(steps(&tools).len(), before);
        assert_eq!(held(&tools, line), RichText::from(LINE));
    }

    #[tokio::test]
    async fn a_word_given_values_of_its_own_is_a_hand_edit_but_a_size_alone_isnt() {
        let (_dir, tools, id) = opened();
        let line = id("line");
        assert!(!edited(&layer(&tools, line)).style);
        let larger = json!({"layer": line, "text": "KNOW", "size_percent": 150});
        style(&tools, larger).await.expect("a larger word");
        assert!(!edited(&layer(&tools, line)).style);
        let bold = json!({"layer": line, "text": "NO", "bold": true});
        style(&tools, bold).await.expect("a bold word");
        assert!(edited(&layer(&tools, line)).style);
    }

    #[tokio::test]
    async fn only_emphasis_reads_the_settings() {
        let (_dir, tools, id) = open(App { broken: true });
        let line = id("line");
        let bold = json!({"layer": line, "text": "NO", "bold": true});
        style(&tools, bold)
            .await
            .expect("bold, with no settings read");
        let stress = json!({"layer": line, "text": "KNOW", "emphasis": true});
        let error = style(&tools, stress)
            .await
            .expect_err("no emphasis to read");
        assert!(error.to_string().contains("pages_per_request"), "{error}");
    }

    #[tokio::test]
    async fn an_unknown_argument_is_refused() {
        let (_dir, _tools, id) = opened();
        let schema = StyleWords.parameters();
        let underlined =
            json!({"layer": id("line"), "text": "NO", "bold": true, "underline": true});
        assert!(serde_json::from_value::<StyleWordsArgs>(underlined.clone()).is_err());
        assert!(crate::tools::refusal(&schema, &underlined).is_some());
        // The schema takes a call in bounds and refuses one out of them.
        let taken = json!({"layer": id("line"), "start": 2, "end": 4, "size_percent": 130, "fill": [1, 0, 0, 1]});
        assert_eq!(crate::tools::refusal(&schema, &taken), None);
        let large = json!({"layer": id("line"), "text": "NO", "size_percent": 500});
        assert!(crate::tools::refusal(&schema, &large).is_some());
    }

    #[test]
    fn styling_words_changes_the_project() {
        assert_ne!(StyleWords.annotations().read_only_hint, Some(true));
        let definitions = Tools::toolset().tool_definitions();
        let tool = definitions
            .iter()
            .find(|tool| tool.name.as_str() == StyleWords::NAME)
            .expect("booharu_style_words is offered");
        assert!(!tool.parameters.to_string().contains("$ref"));
    }
}
