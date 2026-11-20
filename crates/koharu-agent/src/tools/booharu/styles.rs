//! Apply, as the agent calls it: the default text look given again to
//! translations already made, on some blocks, some pages, or the whole
//! project, and how many blocks that would change beforehand, each worked
//! out by `booharu::styles` as the Apply dialog's commands are, under the
//! saved `[booharu-lettering]` and the first of Koharu's Preferred fonts as
//! they are when called. The agent may also name the lettering category, or
//! the balloon shape, every block is lettered as. The look the agent gives
//! is its choice: each block it changes counts as styled by hand afterwards,
//! so automatic steps, and an Apply that leaves hand-edited blocks alone,
//! keep it.
//!
//! And Reset to lettering default, as the app's command does it: chosen
//! blocks and text layers given back the look Booharu letters them in, hand
//! edits and the agent's own look included, which counts as automatic
//! again, with their gradient fills, letter spacing and skew taken off.

use booharu::styles::{self, ResetCounts, StyleCounts, StyleScope};
use booharu_core::{
    attrs::Origin,
    lettering::{self, LetteringCategory},
    settings::LetteringSettings,
    shape::BubbleShape,
    styles::first_font,
};
use koharu_renderer::RendererConfig;
use koharu_scene::LayerId;
use rig_core::{
    completion::ToolAnnotations,
    tool::{Tool, ToolContext, ToolExecutionError},
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::tools::{Tools, failed, project, schema};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct TextStyleArgs {
    /// The blocks to look at: `{"kind": "blocks", "blocks": [...]}`, every
    /// block on `{"kind": "pages", "pages": [...]}`, or every block of
    /// `{"kind": "project"}`.
    scope: StyleScope,
    /// Whether blocks changed by hand take the look too; when false, the
    /// default, they're kept as they are.
    #[serde(default)]
    include_hand_edited: bool,
    /// The lettering category every block is lettered as, in its font,
    /// whatever its own; left out, each block's own while Pick fonts
    /// automatically is on. Not with `shape`.
    #[serde(default)]
    category: Option<LetteringCategory>,
    /// The balloon shape every block is lettered as, as its category:
    /// `round` as Normal Dialogue, `rectangle` as Narration Box, `spiky` as
    /// Shout, and `none`, no balloon, as Floating. Not with `category`.
    #[serde(default)]
    shape: Option<Shape>,
}

/// A balloon's shape, or none.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum Shape {
    Round,
    Rectangle,
    Spiky,
    None,
}

impl TextStyleArgs {
    /// The category the call letters every block as, when it names one.
    fn category(&self) -> Result<Option<LetteringCategory>, ToolExecutionError> {
        let shape = |shape| match shape {
            Shape::Round => Some(BubbleShape::Round),
            Shape::Rectangle => Some(BubbleShape::Rectangle),
            Shape::Spiky => Some(BubbleShape::Spiky),
            Shape::None => None,
        };
        match (self.category, self.shape) {
            (Some(_), Some(_)) => Err(ToolExecutionError::invalid_args(
                "name a category or a shape, not both",
            )),
            (category, None) => Ok(category),
            (None, Some(named)) => Ok(Some(lettering::shape_category(shape(named)))),
        }
    }
}

pub(super) struct ApplyTextStyle;

impl Tool for ApplyTextStyle {
    const NAME: &'static str = "booharu_apply_text_style";
    type Args = TextStyleArgs;
    type Output = StyleCounts;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Give the translations of some blocks, every block on some pages, or every block of the \
         project the default text look again, as one undo step: the default font, the first of \
         Koharu's Preferred fonts, in place of each one's family, or while Pick fonts \
         automatically (`auto`) is on its block's lettering category's font in place of its \
         family, weight, and italic, and with colour detection off the default colours and \
         outline in place of its own, as `booharu-lettering` in booharu_get_settings holds \
         them; with detection on, no colours change. `category`, or `shape`, letters every \
         block as that category instead, in its font as booharu_get_text_styles lists it, \
         whatever the block's own category and whether `auto` is on: use it when view_page \
         shows a box lettered in the wrong kind of font, such as a narration box lettered as \
         speech (`shape` `rectangle`) or speech lettered as a shout (`category` \
         `normal_dialogue`); a category's font never touches the marks on single words. \
         Booharu leaves blocks changed by hand alone, their wording or their look, unless \
         `include_hand_edited` is true, and always leaves locked blocks alone; blocks without \
         a translation have nothing to restyle. Answers how many blocks it `changed`, kept as \
         `kept_hand_edited` (changed by hand, or locked), and found `untranslated`: tell the \
         user how many were kept, and that including hand-edited blocks would restyle those \
         changed by hand too, though never the locked ones. The look it gives counts as chosen \
         by hand, so later automatic steps, booharu_set_lettering_category's included, leave \
         those blocks alone, and restyling them again needs `include_hand_edited`. \
         booharu_count_text_style_targets answers the same numbers beforehand without changing \
         anything."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<TextStyleArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: TextStyleArgs,
    ) -> Result<StyleCounts, Self::Error> {
        let category = args.category()?;
        let tools = project(context)?;
        // On a blocking thread, as `edit` applies, and under one lock, so the
        // blocks counted are the ones changed.
        tokio::task::spawn_blocking(move || -> anyhow::Result<StyleCounts> {
            let (settings, font) = look(&tools)?;
            let mut session = tools.session.lock();
            let (edit, counts) = styles::apply(
                &session.project(),
                &args.scope,
                args.include_hand_edited,
                category,
                &settings,
                font.as_deref(),
                Origin::Agent,
            )?;
            if let Some(edit) = edit {
                session.apply(edit)?;
            }
            Ok(counts)
        })
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
    }
}

pub(super) struct CountTextStyleTargets;

impl Tool for CountTextStyleTargets {
    const NAME: &'static str = "booharu_count_text_style_targets";
    type Args = TextStyleArgs;
    type Output = StyleCounts;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "How many blocks booharu_apply_text_style would change for the same scope, \
         `include_hand_edited`, `category`, and `shape` now, changing nothing: `changed`, the \
         translated blocks that would take the look; `kept_hand_edited`, those it would keep, \
         changed by hand or locked; and `untranslated`, the blocks without a translation. A \
         block whose translation has the look already counts in none."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<TextStyleArgs>()
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations::default().with_read_only(true)
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: TextStyleArgs,
    ) -> Result<StyleCounts, Self::Error> {
        let category = args.category()?;
        let tools = project(context)?;
        tokio::task::spawn_blocking(move || -> anyhow::Result<StyleCounts> {
            let (settings, font) = look(&tools)?;
            let project = tools.session.lock().project();
            styles::count(
                &project,
                &args.scope,
                args.include_hand_edited,
                category,
                &settings,
                font.as_deref(),
            )
        })
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ResetLetteringArgs {
    /// The layers: blocks, whose translations take their default, and text
    /// layers, sound effects' subtitles among them.
    layers: Vec<LayerId>,
}

pub(super) struct ResetLettering;

impl Tool for ResetLettering {
    const NAME: &'static str = "booharu_reset_lettering";
    type Args = ResetLetteringArgs;
    type Output = ResetCounts;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Give some layers their lettering default back, as Reset to lettering default does, as one \
         undo step: a translated block the look a first translation of it gets, at its size now \
         (the colours and outline Koharu measures from the original lettering, with a sound \
         effect's legibility outline, or with colour detection (`detect_colors`) off the default \
         colours and outline, in the default font, or while Pick fonts automatically (`auto`) is \
         on in its lettering category's font), a sound effect's subtitle its subtitle look \
         (`subtitle`), and any other text layer the default look, as `booharu-lettering` sets \
         them; every mark a word carries goes too, and so do the text effects \
         booharu_get_text_effects lists: its own outlines, gradient fills on the box and on its \
         words, and its skew, booharu_set_skew's, which comes off about the point the text \
         leans about; letter spacing goes too. Only the look changes: the words, the size, the \
         frame, the position, the rotation, the alignment, and the rest stay. Translating a \
         block again keeps its look, so \
         this is the way back to the default: use it when the user asks for a box's default look \
         again, or after booharu_set_lettering_category made a block a sound effect or no longer \
         one, whose outline otherwise changes only at its next translation. It replaces a look \
         changed by hand, the user's or booharu_apply_text_style's, and the look it gives counts \
         as automatic. Locked layers, and layers in locked layers, are left alone; a block without \
         a translation has no lettering to reset. Answers how many layers it `changed`, left alone \
         as `locked`, and found `untranslated`: tell the user about any it left alone. A layer \
         with its default already counts in none, and with nothing to change, it makes no undo \
         step."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<ResetLetteringArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: ResetLetteringArgs,
    ) -> Result<ResetCounts, Self::Error> {
        let tools = project(context)?;
        // On a blocking thread, as `edit` applies, and under one lock, so the
        // layers counted are the ones changed.
        tokio::task::spawn_blocking(move || -> anyhow::Result<ResetCounts> {
            let (settings, font) = look(&tools)?;
            let mut session = tools.session.lock();
            let (edit, counts) = styles::reset_lettering(
                &session.project(),
                &args.layers,
                &settings,
                font.as_deref(),
            )?;
            if let Some(edit) = edit {
                session.apply(edit)?;
            }
            Ok(counts)
        })
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
    }
}

/// The default look as the app holds it now, `[booharu-lettering]`, and the
/// default font, the first of Koharu's Preferred fonts, none when they list
/// none; outside the app, both as the file holds them. A section that can't
/// be read is an error, so nothing is restyled by a value never chosen.
pub(super) fn look(tools: &Tools) -> anyhow::Result<(LetteringSettings, Option<String>)> {
    let settings = lettering(tools)?;
    let families = match &tools.booharu {
        Some(host) => host.font_families()?,
        None => koharu_config::get::<RendererConfig>()?.font_families,
    };
    Ok((settings, first_font(&families).map(str::to_owned)))
}

/// `[booharu-lettering]` as the app holds it now, or outside the app as the
/// file holds it, which the SFX mode a run follows is read from; a section
/// that can't be read is an error.
pub(super) fn lettering(tools: &Tools) -> anyhow::Result<LetteringSettings> {
    Ok(match &tools.booharu {
        Some(host) => host.preferences()?.lettering,
        None => koharu_config::get()?,
    })
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use booharu_core::{
        attrs::Auto, edited, lettering::CategoryFont, settings::BooharuPreferences,
    };
    use koharu_pipeline::{Pipeline, PipelineConfig};
    use koharu_renderer::Renderer;
    use koharu_scene::{CharStyle, Content, Created, Edit, LayerId, Rgba, RichText, Session, Span};
    use parking_lot::Mutex;
    use serde_json::json;

    use super::*;
    use crate::tools::booharu::BooharuHost;

    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
    const BLUE: Rgba = Rgba([0.0, 0.0, 1.0, 1.0]);

    /// The app, whose default look is red text with colour detection off,
    /// whose first Preferred font is Anime Ace, and which letters shouts in
    /// a bold Badaboom BB and narration boxes in an italic Comic Neue, with
    /// Pick fonts automatically off.
    struct Red;

    impl BooharuHost for Red {
        fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
            let mut preferences = BooharuPreferences::default();
            preferences.lettering.default.fill = Some(RED);
            preferences.lettering.detect_colors = false;
            let font = |family: &str, bold, italic| CategoryFont {
                family: Some(family.to_owned()),
                bold,
                italic,
            };
            preferences.lettering.categories = BTreeMap::from([
                (LetteringCategory::Shout, font("Badaboom BB", true, false)),
                (
                    LetteringCategory::NarrationBox,
                    font("Comic Neue", false, true),
                ),
            ]);
            Ok(preferences)
        }

        fn save_section(
            &self,
            _: &str,
            _: serde_json::Map<String, Value>,
        ) -> anyhow::Result<BooharuPreferences> {
            anyhow::bail!("nothing here saves settings")
        }

        fn font_families(&self) -> anyhow::Result<Vec<String>> {
            Ok(vec!["Anime Ace".to_owned(), "Wild Words".to_owned()])
        }
    }

    /// A page of blocks lettered in blue, each found and read by Koharu's
    /// stages: `auto`, translated by them, `typed`, translated by hand,
    /// `locked`, translated by them and locked, and `none`, untranslated;
    /// the tools on it, and the blocks' ids.
    fn open() -> (tempfile::TempDir, Tools, impl Fn(&str) -> LayerId) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let found =
            json!({"detection": {"action": "c2pa.created"}, "ocr": {"action": "c2pa.created"}});
        let mut translated = found.clone();
        translated["translation"] = json!({"action": "c2pa.translated"});
        let block = |label: &str, c2pa: &Value, translation: bool, locked: bool| {
            let mut layer = json!({"kind": "block", "locked": locked, "source": {"content": "やあ"}, "attributes": {"c2pa": c2pa}});
            if translation {
                layer["translation"] =
                    json!({"content": "Hi", "style": {"chars": {"size": 32.0, "fill": BLUE}}});
            }
            json!({"op": "add_layer", "id": format!("${label}"), "page": "$page", "index": 0, "layer": layer})
        };
        let ops = [
            json!({"op": "add_page", "id": "$page", "index": 0, "size": {"width": 100, "height": 200}}),
            block("auto", &translated, true, false),
            block("typed", &found, true, false),
            block("locked", &translated, true, true),
            block("none", &found, false, false),
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
                .expect("a block")
        };
        let tools = Tools::new(
            Arc::new(Mutex::new(session)),
            Pipeline::new(PipelineConfig::default()),
            Renderer::new(RendererConfig::default()),
        )
        .with_booharu(Arc::new(Red));
        (dir, tools, id)
    }

    fn args(include_hand_edited: bool) -> TextStyleArgs {
        TextStyleArgs {
            scope: StyleScope::Project,
            include_hand_edited,
            category: None,
            shape: None,
        }
    }

    fn counts(changed: u32, kept_hand_edited: u32, untranslated: u32) -> StyleCounts {
        StyleCounts {
            changed,
            kept_hand_edited,
            untranslated,
        }
    }

    async fn count(tools: &Tools, include_hand_edited: bool) -> StyleCounts {
        CountTextStyleTargets
            .call(&mut tools.tool_context(), args(include_hand_edited))
            .await
            .expect("a count")
    }

    #[tokio::test]
    async fn the_agents_look_keeps_hand_edits_and_counts_as_one_afterwards() {
        let (_dir, tools, id) = open();
        // The typed translation and the locked block are kept.
        assert_eq!(count(&tools, false).await, counts(1, 2, 1));
        let applied = ApplyTextStyle
            .call(&mut tools.tool_context(), args(false))
            .await
            .expect("the look applied");
        assert_eq!(applied, counts(1, 2, 1));

        let project = tools.session.lock().project();
        let chars = |label: &str| {
            let layer = project.layer(id(label)).expect("the block");
            match &layer.content {
                Content::Block(block) => block.translation.clone().expect("a translation"),
                _ => panic!("{label} isn't a block"),
            }
            .style
            .chars
        };
        assert_eq!(
            chars("auto"),
            CharStyle {
                family: Some("Anime Ace".to_owned()),
                size: Some(32.0),
                fill: Some(RED),
                ..CharStyle::default()
            }
        );
        assert_eq!(chars("typed").fill, Some(BLUE));
        assert_eq!(chars("locked").fill, Some(BLUE));
        // The agent chose the look, so it's kept as a hand edit is.
        let auto = project.layer(id("auto")).expect("the block");
        assert_eq!(Auto::of(auto).origin, Origin::Agent);
        assert!(edited(auto).style);
        assert_eq!(count(&tools, false).await, counts(0, 3, 1));
        // Including hand edits restyles the typed one only; the locked block
        // is kept whatever.
        assert_eq!(count(&tools, true).await, counts(1, 1, 1));
    }

    #[tokio::test]
    async fn a_category_or_shape_named_letters_the_blocks_in_its_font_as_the_agents_choice() {
        let (_dir, tools, id) = open();
        let named = |names: &[&str], include_hand_edited, category, shape| TextStyleArgs {
            scope: StyleScope::Blocks {
                blocks: names.iter().map(|name| id(name)).collect(),
            },
            include_hand_edited,
            category,
            shape,
        };
        let shout = |include_hand_edited| {
            named(
                &["auto"],
                include_hand_edited,
                Some(LetteringCategory::Shout),
                None,
            )
        };
        let count = async |args| {
            CountTextStyleTargets
                .call(&mut tools.tool_context(), args)
                .await
                .expect("a count")
        };
        // Shout's font, though Pick fonts automatically is off.
        assert_eq!(count(shout(false)).await, counts(1, 0, 0));
        let applied = ApplyTextStyle
            .call(&mut tools.tool_context(), shout(false))
            .await
            .expect("the look applied");
        assert_eq!(applied, counts(1, 0, 0));
        let project = tools.session.lock().project();
        let layer = project.layer(id("auto")).expect("the block");
        let Content::Block(block) = &layer.content else {
            panic!("auto isn't a block");
        };
        assert_eq!(
            block
                .translation
                .as_ref()
                .expect("a translation")
                .style
                .chars,
            CharStyle {
                family: Some("Badaboom BB".to_owned()),
                weight: Some(700),
                size: Some(32.0),
                fill: Some(RED),
                ..CharStyle::default()
            }
        );
        // The agent chose it, so it's kept as a hand edit is, and has the
        // look already when hand edits are included.
        assert_eq!(Auto::of(layer).origin, Origin::Agent);
        assert!(edited(layer).style);
        assert_eq!(count(shout(false)).await, counts(0, 1, 0));
        assert_eq!(count(shout(true)).await, counts(0, 0, 0));

        // A rectangular balloon's shape letters as a narration box.
        let boxed = named(&["typed"], true, None, Some(Shape::Rectangle));
        assert_eq!(
            ApplyTextStyle
                .call(&mut tools.tool_context(), boxed)
                .await
                .expect("the look applied"),
            counts(1, 0, 0)
        );
        let project = tools.session.lock().project();
        let layer = project.layer(id("typed")).expect("the block");
        let Content::Block(block) = &layer.content else {
            panic!("typed isn't a block");
        };
        let chars = &block
            .translation
            .as_ref()
            .expect("a translation")
            .style
            .chars;
        assert_eq!(
            (chars.family.as_deref(), chars.weight, chars.italic),
            (Some("Comic Neue"), None, Some(true))
        );

        // Naming both is refused before anything is counted or changed.
        let steps = tools.session.lock().history().steps.len();
        let both = || {
            named(
                &["auto"],
                true,
                Some(LetteringCategory::Shout),
                Some(Shape::Spiky),
            )
        };
        let error = ApplyTextStyle
            .call(&mut tools.tool_context(), both())
            .await
            .expect_err("refused");
        assert_eq!(error.kind(), rig_core::tool::ToolErrorKind::InvalidArgs);
        let error = CountTextStyleTargets
            .call(&mut tools.tool_context(), both())
            .await
            .expect_err("refused");
        assert_eq!(error.kind(), rig_core::tool::ToolErrorKind::InvalidArgs);
        assert_eq!(tools.session.lock().history().steps.len(), steps);
    }

    #[test]
    fn each_shape_letters_as_the_category_its_balloon_gives() {
        let shaped = |shape| {
            TextStyleArgs {
                shape: Some(shape),
                ..args(false)
            }
            .category()
            .expect("a category")
        };
        assert_eq!(
            shaped(Shape::Round),
            Some(LetteringCategory::NormalDialogue)
        );
        assert_eq!(
            shaped(Shape::Rectangle),
            Some(LetteringCategory::NarrationBox)
        );
        assert_eq!(shaped(Shape::Spiky), Some(LetteringCategory::Shout));
        assert_eq!(shaped(Shape::None), Some(LetteringCategory::Floating));
        assert_eq!(args(false).category().expect("no category"), None);
        // What the schema takes.
        let schema = ApplyTextStyle.parameters();
        let project = json!({"kind": "project"});
        for args in [
            json!({"scope": project, "category": "whisper"}),
            json!({"scope": project, "shape": "none"}),
        ] {
            assert_eq!(crate::tools::refusal(&schema, &args), None, "{args}");
        }
        for args in [
            json!({"scope": project, "category": "automatic"}),
            json!({"scope": project, "shape": "oval"}),
        ] {
            assert!(crate::tools::refusal(&schema, &args).is_some(), "{args}");
        }
    }

    #[tokio::test]
    async fn a_reset_gives_each_layer_its_lettering_default_as_one_automatic_undo_step() {
        let (_dir, tools, id) = open();
        // The agent letters `auto` as a shout, a look of its own choosing, a
        // word of `typed` is marked, and a text layer is set in a font of
        // its own.
        let shout = TextStyleArgs {
            scope: StyleScope::Blocks {
                blocks: vec![id("auto")],
            },
            include_hand_edited: false,
            category: Some(LetteringCategory::Shout),
            shape: None,
        };
        ApplyTextStyle
            .call(&mut tools.tool_context(), shout)
            .await
            .expect("the shout's look");
        let page = tools
            .session
            .lock()
            .project()
            .pages()
            .next()
            .expect("the page")
            .id;
        let ops = [
            json!({"op": "set_rich_text", "layer": id("typed"), "spans": [
                {"insert": "H", "attributes": {"italic": true}},
                {"insert": "i"},
            ]}),
            json!({"op": "add_layer", "id": "$note", "page": page, "index": 0, "layer": {
                "kind": "text",
                "content": "Note",
                "style": {"chars": {"family": "Wild Words", "size": 20.0}},
            }}),
        ];
        let change = tools
            .session
            .lock()
            .apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(|op| serde_json::from_value(op).expect("an op"))
                    .collect(),
            })
            .expect("the edits");
        let Some(&Created::Layer { id: note, .. }) = change.created.first() else {
            panic!("{change:?}");
        };
        let steps = tools.session.lock().history().steps.len();

        let named = vec![
            id("auto"),
            id("typed"),
            id("locked"),
            id("none"),
            note,
            id("auto"),
        ];
        let reset = async |layers: Vec<LayerId>| {
            ResetLettering
                .call(&mut tools.tool_context(), ResetLetteringArgs { layers })
                .await
                .expect("the reset")
        };
        let answer = |changed| ResetCounts {
            changed,
            locked: 1,
            untranslated: 1,
        };
        assert_eq!(reset(named.clone()).await, answer(3));
        let history = tools.session.lock().history();
        assert_eq!(history.steps.len(), steps + 1);
        assert_eq!(
            history.steps.last().map(String::as_str),
            Some(styles::RESET_LABEL)
        );

        let project = tools.session.lock().project();
        let translation = |name: &str| match &project.layer(id(name)).expect("the block").content {
            Content::Block(block) => block.translation.clone().expect("a translation"),
            _ => panic!("{name} isn't a block"),
        };
        // The default look, the shout's font gone, the size kept.
        let default = |size| CharStyle {
            family: Some("Anime Ace".to_owned()),
            size: Some(size),
            fill: Some(RED),
            ..CharStyle::default()
        };
        assert_eq!(translation("auto").style.chars, default(32.0));
        assert_eq!(translation("typed").style.chars, default(32.0));
        // The word's mark goes, and its words stay.
        assert_eq!(
            translation("typed").content,
            RichText(vec![Span {
                insert: "Hi".to_owned(),
                attributes: CharStyle::default(),
            }])
        );
        assert_eq!(translation("locked").style.chars.fill, Some(BLUE));
        // The look counts as automatic again, not as the agent's choice.
        let auto = project.layer(id("auto")).expect("the block");
        assert_eq!(Auto::of(auto).origin, Origin::Automatic);
        assert!(!edited(auto).style);
        let Content::Text(text) = &project.layer(note).expect("the note").content else {
            panic!("the note isn't a text");
        };
        assert_eq!(text.style.chars, default(20.0));

        // With every layer at its default, nothing changes: no undo step.
        assert_eq!(reset(named).await, answer(0));
        assert_eq!(tools.session.lock().history().steps.len(), steps + 1);
    }

    #[test]
    fn a_reset_changes_the_project() {
        assert_ne!(ResetLettering.annotations().read_only_hint, Some(true));
        let schema = ResetLettering.parameters();
        assert_eq!(crate::tools::refusal(&schema, &json!({"layers": []})), None);
        assert!(crate::tools::refusal(&schema, &json!({"blocks": []})).is_some());
    }

    #[test]
    fn an_unknown_argument_is_refused() {
        let args =
            json!({"scope": {"kind": "project"}, "include_hand_edited": true, "force": true});
        assert!(serde_json::from_value::<TextStyleArgs>(args).is_err());
        let args = json!({"scope": {"kind": "pages", "pages": []}});
        let read = serde_json::from_value::<TextStyleArgs>(args).expect("the arguments");
        assert!(!read.include_hand_edited);
        // A scope naming pages it doesn't take would restyle the whole
        // project, so the schema refuses it before the call.
        let schema = ApplyTextStyle.parameters();
        let pages = json!({"scope": {"kind": "project", "pages": []}});
        assert!(crate::tools::refusal(&schema, &pages).is_some());
        let project = json!({"scope": {"kind": "project"}});
        assert_eq!(crate::tools::refusal(&schema, &project), None);
    }
}
