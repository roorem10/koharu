//! Lettering categories, as the agent works with them: blocks' categories
//! set as the agent's labels, each change worked out by `booharu::lettering`
//! as the app's command works it out, and the font each category letters
//! its text in, with each block's category, why it has it, and its
//! balloon's shape, under the saved `[booharu-lettering]` and the first of
//! Koharu's Preferred fonts as they are when called. The agent's labels
//! count as automatic ones (T18): they never replace a category the user
//! chose, and never restyle a block whose look was changed by hand.

use booharu::lettering::{self, BlockLettering, CategoryCounts, Chooser, ResolvedFont};
use booharu_core::{lettering::LetteringCategory, settings::DefaultTextStyle};
use koharu_scene::{LayerId, PageId};
use rig_core::{
    completion::ToolAnnotations,
    tool::{Tool, ToolContext, ToolExecutionError},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::styles::look;
use crate::tools::{failed, project, schema};

/// A category to letter blocks in, by its id, or `automatic`.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(super) enum Choice {
    Category(LetteringCategory),
    Automatic(Automatic),
}

/// `automatic`: the category each block's own rules give.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum Automatic {
    Automatic,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SetLetteringCategoryArgs {
    /// The blocks.
    blocks: Vec<LayerId>,
    /// The category's id, or `automatic` to take back your label.
    category: Choice,
    /// How sure you are of the category, from 0 to 1; left out counts as
    /// sure.
    #[serde(default)]
    #[schemars(range(min = 0.0, max = 1.0))]
    confidence: Option<f32>,
}

pub(super) struct SetLetteringCategory;

impl Tool for SetLetteringCategory {
    const NAME: &'static str = "booharu_set_lettering_category";
    type Args = SetLetteringCategoryArgs;
    type Output = CategoryCounts;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Set the lettering category of some blocks, which decides the font their translations \
         are lettered in while Pick fonts automatically (`auto` in `booharu-lettering`) is on, \
         as one undo step. Use it when you can tell what kind of line a block holds and its box \
         can't, for example when view_page or the source text shows a whisper, a thought, a \
         moan, or a title lettered as ordinary speech; booharu_get_text_styles tells each \
         block's category and why. `category` is a category's id, or `automatic` to take back \
         your label, so each block takes the category its own rules give. Your label counts as an \
         automatic one: it never replaces a category the user chose (`kept_user_label`), and a \
         block whose look was changed by hand, by the user or by booharu_apply_text_style, \
         takes your label but keeps its font (`kept_hand_edited`). A presentation category \
         (`title`, `phone_text`, `sfx`, `handwritten`, `sign`, `electronic_voice`) wins at any \
         `confidence`; a delivery category (`thoughts`, `weak_shout`, `moan`, `whisper`, `shy`, \
         `crying`, `menacing`, `shout`) wins over small text and the balloon's shape only when \
         `confidence` is at least `model_confidence` in `booharu-lettering`, and never over a \
         block Koharu detected as a sound effect. `normal_dialogue`, `floating`, `small_text`, \
         and `narration_box` come from a block's balloon and text size, so a label of yours \
         can't give them: booharu_apply_text_style with that `category`, or with a `shape`, \
         letters a block as one of those. With `auto` off only the labels change; with it on, \
         each translated block takes its category's font, and a block without a translation \
         takes it once translated. Locked blocks, and blocks in locked layers, are left out. \
         Answers how many blocks it `changed`, left out as `locked`, `kept_user_label`, \
         `kept_hand_edited`, and found `untranslated`, which take only the label; tell the \
         user about any it left out."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<SetLetteringCategoryArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: SetLetteringCategoryArgs,
    ) -> Result<CategoryCounts, Self::Error> {
        let category = match args.category {
            Choice::Automatic(Automatic::Automatic) => None,
            Choice::Category(category) if category.presentation() || category.delivery() => {
                Some(category)
            }
            Choice::Category(category) => {
                return Err(ToolExecutionError::invalid_args(format!(
                    "a label of yours never letters a block as {}, which comes from its \
                     balloon's shape and its text's size; booharu_apply_text_style with \
                     `category` `{}` letters the blocks in its font",
                    category.name(),
                    category.as_str()
                )));
            }
        };
        let tools = project(context)?;
        let blocks = args.blocks;
        let chooser = Chooser::Model {
            confidence: args.confidence,
        };
        // On a blocking thread, as `edit` applies, and under one lock, so the
        // blocks counted are the ones changed.
        tokio::task::spawn_blocking(move || -> anyhow::Result<CategoryCounts> {
            let (settings, font) = look(&tools)?;
            let mut session = tools.session.lock();
            let (edit, counts) = lettering::set_category(
                &session.project(),
                &blocks,
                category,
                chooser,
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

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct GetTextStylesArgs {
    /// A page whose blocks to answer too, in reading order; not with
    /// `blocks`.
    #[serde(default)]
    page: Option<PageId>,
    /// Blocks to answer too, in the order named; not with `page`.
    #[serde(default)]
    blocks: Option<Vec<LayerId>>,
}

/// How Booharu letters text, as booharu_get_text_styles answers it.
#[derive(Debug, Serialize)]
pub(super) struct TextStyles {
    /// The default look new text gets.
    default: DefaultTextStyle,
    /// Whether a new translation keeps the colours Koharu measured.
    detect_colors: bool,
    /// The default font, the first of Koharu's Preferred fonts.
    font: Option<String>,
    /// Pick fonts automatically.
    auto: bool,
    /// The font each category letters its text in.
    categories: Vec<ResolvedFont>,
    /// The blocks asked about, when any were.
    #[serde(skip_serializing_if = "Option::is_none")]
    blocks: Option<Vec<BlockLettering>>,
}

pub(super) struct GetTextStyles;

impl Tool for GetTextStyles {
    const NAME: &'static str = "booharu_get_text_styles";
    type Args = GetTextStylesArgs;
    type Output = TextStyles;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "How Booharu letters text, and with `page` or `blocks` each block's lettering category, \
         changing nothing. Use it before booharu_set_lettering_category or \
         booharu_apply_text_style, for example after view_page shows a box lettered in the \
         wrong kind of font, to see why the block has its category and the font that gives. \
         Answers `default`, the default look new text gets: its `fill` and `stroke` colours as \
         [red, green, blue, alpha] from 0 to 1, or null for Koharu's black and no outline, and \
         its `stroke_width`; `detect_colors`, whether a new translation keeps the colours \
         Koharu measured rather than the default ones; `font`, the default font, the first of \
         Koharu's Preferred fonts; `auto`, Pick fonts automatically, whether each block's \
         translation is lettered in its category's font; and `categories`, each of the 18 \
         categories in Settings' order, its `category` id and English `name`, with the font it \
         letters its text in, its `family`, `bold`, and `italic`, and `from`: its own setting \
         (`own`), Normal Dialogue's (`normal_dialogue`), or neither, the default font, neither \
         bold nor italic (`default`). With `blocks`, as named, or `page`, in reading order, \
         `blocks` answers each block's `category` with its `reason`: `user`, the user chose it; \
         `model`, a model or you chose it; `sfx`, Koharu detected a sound effect; \
         `small_text`, its text is small; `shape`, its balloon's shape; or `floating`, it's in \
         no balloon; its `label`, the category chosen for it with its `source`, `user` or \
         `model`, and a model's `confidence`, null without one; its balloon's `shape`, null \
         outside any balloon: its `shape`, `round`, `rectangle`, or `spiky`, and the \
         `measures` that decided it, `rectangularity` and `solidity` from 0 to 1, and \
         `spikes`, how many dips in its outline go deeper than 8% of the short side of the \
         smallest rectangle around it; and `hand_edited`, whether its translation's wording or \
         look was changed by hand, booharu_get_project_overview telling which: \
         booharu_set_lettering_category keeps the font of a block whose look was. To change \
         the settings, use booharu_set_settings on `booharu-lettering`."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<GetTextStylesArgs>()
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations::default().with_read_only(true)
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: GetTextStylesArgs,
    ) -> Result<TextStyles, Self::Error> {
        if args.page.is_some() && args.blocks.is_some() {
            return Err(ToolExecutionError::invalid_args(
                "name a page or blocks, not both",
            ));
        }
        let tools = project(context)?;
        tokio::task::spawn_blocking(move || -> anyhow::Result<TextStyles> {
            let (settings, font) = look(&tools)?;
            let project = tools.session.lock().project();
            let blocks = match (args.page, args.blocks) {
                (Some(page), _) => Some(lettering::info(&project, page, &settings)?),
                (None, Some(blocks)) => Some(lettering::blocks(&project, &blocks, &settings)?),
                (None, None) => None,
            };
            Ok(TextStyles {
                categories: lettering::fonts(&settings, font.as_deref()),
                default: settings.default,
                detect_colors: settings.detect_colors,
                auto: settings.auto,
                font,
                blocks,
            })
        })
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use booharu_core::{
        attrs::{self, Auto, Lettering, LetteringSource, Origin},
        edited,
        lettering::CategoryFont,
        settings::{BooharuPreferences, LetteringSettings},
    };
    use koharu_pipeline::{Pipeline, PipelineConfig};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::{Content, Created, Edit, Layer, Session};
    use parking_lot::Mutex;
    use rig_core::tool::ToolErrorKind;
    use serde_json::{Map, json};

    use super::*;
    use crate::tools::{Tools, booharu::BooharuHost};

    /// The default font, the first of Koharu's Preferred fonts.
    const FONT: &str = "Anime Ace";

    /// The look with Pick fonts automatically on, Normal Dialogue in Comic
    /// Neue, and Shout in a bold Badaboom BB.
    fn lettered() -> LetteringSettings {
        let font = |family: &str, bold| CategoryFont {
            family: Some(family.to_owned()),
            bold,
            italic: false,
        };
        LetteringSettings {
            auto: true,
            categories: BTreeMap::from([
                (LetteringCategory::NormalDialogue, font("Comic Neue", false)),
                (LetteringCategory::Shout, font("Badaboom BB", true)),
            ]),
            ..LetteringSettings::default()
        }
    }

    /// The app, lettering as [`lettered`] does with [`FONT`] the default
    /// font.
    struct Lettered;

    impl BooharuHost for Lettered {
        fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
            Ok(BooharuPreferences {
                lettering: lettered(),
                ..BooharuPreferences::default()
            })
        }

        fn save_section(
            &self,
            _: &str,
            _: Map<String, Value>,
        ) -> anyhow::Result<BooharuPreferences> {
            anyhow::bail!("nothing here saves settings")
        }

        fn font_families(&self) -> anyhow::Result<Vec<String>> {
            Ok(vec![FONT.to_owned(), "Wild Words".to_owned()])
        }
    }

    /// A page 400 by 400 with a round balloon holding the block `a` by its
    /// centre, and outside it the blocks `b`, `c`, which the user lettered
    /// as a whisper, `d`, whose translation was typed by hand, and `e`,
    /// locked, each found and read by Koharu's stages, at size 32, and
    /// translated in the default font, all but `d` by them; the tools on
    /// it, the page, and the blocks' ids.
    fn open() -> (tempfile::TempDir, Tools, PageId, impl Fn(&str) -> LayerId) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let block = |label: &str, (x, y): (f64, f64), by_stages: bool, attributes: Value| {
            let mut c2pa = json!({
                "detection": {"action": "c2pa.created"},
                "ocr": {"action": "c2pa.created"},
            });
            if by_stages {
                c2pa["translation"] = json!({"action": "c2pa.translated"});
            }
            let mut attributes = attributes;
            attributes["c2pa"] = c2pa;
            let layer = json!({"kind": "block", "locked": label == "e", "source": {
                "frame": {"box": {"bounds": {"x": x, "y": y, "width": 60.0, "height": 60.0}}},
                "content": "どうして",
                "style": {"chars": {"size": 32.0}},
            }, "translation": {
                "content": "Why?",
                "style": {"chars": {"size": 32.0, "family": FONT}},
            }, "attributes": attributes});
            json!({"op": "add_layer", "id": format!("${label}"), "page": "$p", "index": 0, "layer": layer})
        };
        let whisper = json!({"booharu": {"lettering": {"category": "whisper", "source": "user"}}});
        let balloon = "M10 100 C10 20 190 20 190 100 C190 180 10 180 10 100 Z";
        let ops = [
            json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}),
            json!({"op": "add_layer", "id": "$balloon", "page": "$p", "index": 0,
                "layer": {"kind": "balloon", "outline": balloon}}),
            block("a", (70.0, 70.0), true, json!({})),
            block("b", (250.0, 20.0), true, json!({})),
            block("c", (250.0, 120.0), true, whisper),
            block("d", (250.0, 220.0), false, json!({})),
            block("e", (250.0, 320.0), true, json!({})),
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
        let page = change.pages.added[0];
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
        .with_booharu(Arc::new(Lettered));
        (dir, tools, page, id)
    }

    /// `args` read as the tool reads them, once its schema took them.
    fn read<T: serde::de::DeserializeOwned>(schema: &Value, args: Value) -> T {
        assert_eq!(crate::tools::refusal(schema, &args), None);
        serde_json::from_value(args).expect("the arguments")
    }

    async fn set(tools: &Tools, args: Value) -> Result<CategoryCounts, ToolExecutionError> {
        let args = read(&SetLetteringCategory.parameters(), args);
        SetLetteringCategory
            .call(&mut tools.tool_context(), args)
            .await
    }

    async fn styles(tools: &Tools, args: Value) -> Result<Value, ToolExecutionError> {
        let args = read(&GetTextStyles.parameters(), args);
        let styles = GetTextStyles.call(&mut tools.tool_context(), args).await?;
        Ok(serde_json::to_value(styles).expect("the answer as JSON"))
    }

    fn layer(tools: &Tools, id: LayerId) -> Layer {
        tools
            .session
            .lock()
            .project()
            .layer(id)
            .expect("the block")
            .clone()
    }

    /// The family, weight, and italic of the translation of `id`.
    fn font(tools: &Tools, id: LayerId) -> (Option<String>, Option<u16>, Option<bool>) {
        match layer(tools, id).content {
            Content::Block(block) => {
                let chars = block.translation.expect("a translation").style.chars;
                (chars.family, chars.weight, chars.italic)
            }
            _ => panic!("{id} isn't a block"),
        }
    }

    fn named(family: &str, weight: Option<u16>) -> (Option<String>, Option<u16>, Option<bool>) {
        (Some(family.to_owned()), weight, None)
    }

    fn model(category: LetteringCategory, confidence: Option<f32>) -> Option<Lettering> {
        Some(Lettering {
            category,
            source: LetteringSource::Model,
            confidence,
            role: None,
        })
    }

    #[tokio::test]
    async fn the_agents_category_gives_the_fonts_the_apps_command_gives() {
        let (_dir, tools, _, id) = open();
        let (_twin_dir, twin, _, twin_id) = open();
        let counts = set(
            &tools,
            json!({"blocks": [id("a"), id("b")], "category": "shout"}),
        )
        .await
        .expect("the category set");
        assert_eq!(
            counts,
            CategoryCounts {
                changed: 2,
                ..CategoryCounts::default()
            }
        );
        // The app's command, as the user picks the same category on a twin
        // of the project, under the same settings.
        let (settings, default) = look(&twin).expect("the look");
        {
            let mut session = twin.session.lock();
            let (edit, _) = lettering::set_category(
                &session.project(),
                &[twin_id("a"), twin_id("b")],
                Some(LetteringCategory::Shout),
                Chooser::User,
                &settings,
                default.as_deref(),
            )
            .expect("the edit");
            session.apply(edit.expect("a change")).expect("applied");
        }
        for name in ["a", "b"] {
            assert_eq!(font(&tools, id(name)), named("Badaboom BB", Some(700)));
            assert_eq!(font(&tools, id(name)), font(&twin, twin_id(name)), "{name}");
        }
        // The agent's label counts as an automatic one, and so does the
        // font it gave.
        let a = layer(&tools, id("a"));
        assert_eq!(
            attrs::get::<Lettering>(&a),
            model(LetteringCategory::Shout, None)
        );
        assert_eq!(Auto::of(&a).origin, Origin::Automatic);
        assert!(!edited(&a).style);
    }

    #[tokio::test]
    async fn an_agents_label_never_replaces_a_users_nor_restyles_a_hand_changed_font() {
        let (_dir, tools, _, id) = open();
        let before = |name: &str| font(&tools, id(name));
        let (c, d) = (before("c"), before("d"));
        let all: Vec<LayerId> = ["a", "b", "c", "d", "e"].map(&id).into();
        let counts = set(
            &tools,
            json!({"blocks": all, "category": "shout", "confidence": 0.9}),
        )
        .await
        .expect("the category set");
        assert_eq!(
            counts,
            CategoryCounts {
                changed: 3,
                locked: 1,
                kept_user_label: 1,
                kept_hand_edited: 1,
                untranslated: 0,
            }
        );
        let label = |name: &str| attrs::get::<Lettering>(&layer(&tools, id(name)));
        assert_eq!(label("a"), model(LetteringCategory::Shout, Some(0.9)));
        assert_eq!(font(&tools, id("a")), named("Badaboom BB", Some(700)));
        // The user's whisper stays, in its font.
        assert_eq!(
            label("c").map(|label| (label.category, label.source)),
            Some((LetteringCategory::Whisper, LetteringSource::User))
        );
        assert_eq!(font(&tools, id("c")), c);
        // The typed translation takes the label, but keeps its font.
        assert_eq!(label("d"), model(LetteringCategory::Shout, Some(0.9)));
        assert_eq!(font(&tools, id("d")), d);
        assert_eq!(label("e"), None);

        // Taking the labels back clears only the agent's: `a` is lettered
        // as its round balloon says again.
        let counts = set(
            &tools,
            json!({"blocks": [id("a"), id("c"), id("d")], "category": "automatic"}),
        )
        .await
        .expect("the labels taken back");
        assert_eq!(
            counts,
            CategoryCounts {
                changed: 2,
                kept_user_label: 1,
                kept_hand_edited: 1,
                ..CategoryCounts::default()
            }
        );
        assert_eq!(label("a"), None);
        assert_eq!(font(&tools, id("a")), named("Comic Neue", None));
        assert_eq!(
            label("c").map(|label| label.source),
            Some(LetteringSource::User)
        );
        assert_eq!(label("d"), None);
        assert_eq!(font(&tools, id("d")), d);
    }

    #[tokio::test]
    async fn a_category_only_a_blocks_rules_give_is_refused_and_changes_nothing() {
        let (_dir, tools, _, id) = open();
        let steps = tools.session.lock().history().steps.len();
        for category in ["normal_dialogue", "floating", "small_text", "narration_box"] {
            let error = set(&tools, json!({"blocks": [id("b")], "category": category}))
                .await
                .expect_err("refused");
            assert_eq!(error.kind(), ToolErrorKind::InvalidArgs, "{category}");
            assert!(
                error.message().contains("booharu_apply_text_style"),
                "{error}"
            );
        }
        assert_eq!(tools.session.lock().history().steps.len(), steps);
        // What the schema takes, and what it refuses before the call.
        let schema = SetLetteringCategory.parameters();
        for args in [
            json!({"blocks": [], "category": "tone"}),
            json!({"blocks": [], "category": "shout", "confidence": 1.5}),
            json!({"blocks": [], "category": "shout", "force": true}),
            json!({"category": "shout"}),
        ] {
            assert!(crate::tools::refusal(&schema, &args).is_some(), "{args}");
        }
        let automatic: SetLetteringCategoryArgs =
            read(&schema, json!({"blocks": [], "category": "automatic"}));
        assert!(matches!(
            automatic.category,
            Choice::Automatic(Automatic::Automatic)
        ));
        let moan: SetLetteringCategoryArgs = read(
            &schema,
            json!({"blocks": [], "category": "moan", "confidence": 0.5}),
        );
        assert!(matches!(
            moan.category,
            Choice::Category(LetteringCategory::Moan)
        ));
        assert_eq!(moan.confidence, Some(0.5));
    }

    #[tokio::test]
    async fn the_text_styles_answer_each_blocks_category_with_why_and_its_balloons_shape() {
        let (_dir, tools, page, id) = open();
        let answer = styles(&tools, json!({"page": page}))
            .await
            .expect("the styles");
        assert_eq!(answer["auto"], json!(true));
        assert_eq!(answer["font"], json!(FONT));
        assert_eq!(answer["detect_colors"], json!(true));
        let categories = answer["categories"].as_array().expect("the categories");
        assert_eq!(categories.len(), 18);
        let category = |name: &str| {
            categories
                .iter()
                .find(|font| font["category"] == json!(name))
                .cloned()
                .expect("the category")
        };
        assert_eq!(
            category("shout"),
            json!({"category": "shout", "name": "Shout", "family": "Badaboom BB",
                "bold": true, "italic": false, "from": "own"})
        );
        assert_eq!(category("whisper")["family"], json!("Comic Neue"));
        assert_eq!(category("whisper")["from"], json!("normal_dialogue"));

        let blocks = answer["blocks"].as_array().expect("the blocks");
        assert_eq!(blocks.len(), 5);
        let block = |name: &str| {
            blocks
                .iter()
                .find(|block| block["block"] == json!(id(name)))
                .cloned()
                .expect("the block")
        };
        // In a round balloon, so lettered as speech.
        let a = block("a");
        assert_eq!(a["category"], json!("normal_dialogue"));
        assert_eq!(a["reason"], json!("shape"));
        assert_eq!(a["label"], Value::Null);
        assert_eq!(a["shape"]["shape"], json!("round"));
        let measures = &a["shape"]["measures"];
        for measure in ["rectangularity", "solidity"] {
            let value = measures[measure].as_f64().expect("a measure");
            assert!(value > 0.0 && value <= 1.0, "{measure} {value}");
        }
        assert_eq!(measures["spikes"], json!(0));
        assert_eq!(a["hand_edited"], json!(false));
        // In no balloon.
        let b = block("b");
        assert_eq!(
            (&b["category"], &b["reason"], &b["shape"]),
            (&json!("floating"), &json!("floating"), &Value::Null)
        );
        // The user's choice.
        let c = block("c");
        assert_eq!(
            (&c["category"], &c["reason"]),
            (&json!("whisper"), &json!("user"))
        );
        assert_eq!(c["label"], json!({"category": "whisper", "source": "user"}));
        assert_eq!(block("d")["hand_edited"], json!(true));

        // Blocks named, once each in the order named.
        let named = styles(&tools, json!({"blocks": [id("c"), id("a"), id("c")]}))
            .await
            .expect("the styles");
        let order: Vec<&Value> = named["blocks"]
            .as_array()
            .expect("the blocks")
            .iter()
            .map(|block| &block["block"])
            .collect();
        assert_eq!(order, [&json!(id("c")), &json!(id("a"))]);
        // Neither, the settings alone; both, refused.
        let settings = styles(&tools, json!({})).await.expect("the styles");
        assert!(settings.get("blocks").is_none(), "{settings}");
        let both = styles(&tools, json!({"page": page, "blocks": [id("a")]}))
            .await
            .expect_err("refused");
        assert_eq!(both.kind(), ToolErrorKind::InvalidArgs);
    }

    #[test]
    fn reading_the_text_styles_changes_nothing() {
        assert_eq!(GetTextStyles.annotations().read_only_hint, Some(true));
        assert_ne!(
            SetLetteringCategory.annotations().read_only_hint,
            Some(true)
        );
    }
}
