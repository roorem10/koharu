//! The project overview: what an agent needs to work on the open project, in
//! one read, beside `get_project`'s pages and `get_page`'s layers.

use std::collections::BTreeMap;

use anyhow::Context as _;
use booharu_core::{
    Edited,
    attrs::{self, Attrs},
    edited,
    settings::{BooharuPreferences, LetteringSettings, SfxMode, SubtitleStyle},
};
use koharu_config::secrets::SecretString;
use koharu_pipeline::{PipelineConfig, TranslatorConfig};
use koharu_renderer::{RendererConfig, fonts};
use koharu_scene::{Content, LayerId, PageId, Project, Rgba, Size};
use rig_core::{
    completion::ToolAnnotations,
    tool::{Tool, ToolContext, ToolExecutionError},
};
use serde::Serialize;
use serde_json::Value;

use crate::tools::{NoArgs, failed, project, schema};

pub(super) struct GetProjectOverview;

impl Tool for GetProjectOverview {
    const NAME: &'static str = "booharu_get_project_overview";
    type Args = NoArgs;
    type Output = Overview;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "What working on the project needs, in one read: each page in order with its id, its \
         size, how many blocks it has and how many of them have a translation with some text, \
         Booharu's attributes of each of its layers that has any, by layer id, and as \
         `hand_edited` the parts of each block changed by hand, by layer id, only for blocks \
         with any: `source`, its wording, `text`, its translation, `style`, its translation's \
         look, words given values of their own among it but for their size and the words a \
         translation run stressed, also when the agent chose it with booharu_apply_text_style \
         or booharu_style_words, and `drawn`, a block drawn by hand rather than detected (a \
         style set on a block before it was translated is a `style` edit, never `text`: its \
         empty placeholder is no translation, so the block stays untranslated); the \
         translator's model, target language, and instructions, with `connected`: whether the \
         model's vendor has its API key or sign-in stored, as the Providers settings show it, \
         never the key itself (Koharu's own models need neither, nor may a server named by its \
         base URL); the typesetting defaults, such as the font families a text falls back on; \
         the name of every font family a text can be set in; as \
         `new_text` the look new text gets: the default `font`, the first of Koharu's Preferred \
         fonts, the default `fill` and `stroke` colours as [red, green, blue, alpha] from 0 to \
         1, null for Koharu's black and no outline, the outline's `stroke_width`, \
         `detect_colors`, whether a new translation keeps the colours Koharu measured, and \
         `auto`, Pick fonts automatically, whether a new translation is lettered in its \
         block's lettering category's font instead of `font`, as booharu_get_text_styles \
         answers each category's font and each block's category, `sfx`, which sound effects a \
         run translates (`skip`, none, `inside_bubbles`, those inside a speech balloon, or \
         `translate_all`), the others keeping the original art, and `subtitle`, the look of \
         the line booharu_add_sfx_subtitle writes beside a sound effect: its `family`, null \
         for `font`, its `size` as a share of the page's shorter side, and its `fill` and \
         `stroke` colours, the outline a ninth of its size wide; and Booharu's settings. A \
         part that can't be read gives its error instead."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<NoArgs>()
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations::default().with_read_only(true)
    }

    async fn call(&self, context: &mut ToolContext, _: NoArgs) -> Result<Overview, Self::Error> {
        let tools = project(context)?;
        // The bundled catalog downloads the first time it's read, so the list
        // fails offline until it has.
        let fonts = fonts::list()
            .await
            .map(|families| families.into_iter().map(|family| family.name).collect());
        tokio::task::spawn_blocking(move || {
            let project = tools.session.lock().project();
            let new_text = super::styles::look(&tools).map(new_text).into();
            Ok(Overview {
                pages: pages(&project).map_err(failed)?,
                translator: koharu_config::get::<PipelineConfig>()
                    .map(|pipeline| {
                        let settings = pipeline.translator;
                        let connected = connected(&settings.model, koharu_config::secrets::get);
                        Translator {
                            settings,
                            connected: connected.into(),
                        }
                    })
                    .into(),
                typesetting: koharu_config::get::<RendererConfig>().into(),
                fonts: fonts.into(),
                new_text,
                settings: tools.booharu.as_ref().map(|host| host.preferences().into()),
            })
        })
        .await
        .map_err(failed)?
    }
}

/// The overview. Its settings are read as the app saved them, which is what
/// the pipeline and the renderer run with; no part holds a secret.
#[derive(Serialize)]
pub(super) struct Overview {
    pages: Vec<PageOverview>,
    translator: Part<Translator>,
    typesetting: Part<RendererConfig>,
    fonts: Part<Vec<String>>,
    new_text: Part<NewText>,
    /// Booharu's settings sections by table name, from the app; none outside
    /// it, as in the `serve` example.
    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<Part<BooharuPreferences>>,
}

/// The look new text gets now, a new translation's and the T tool's text's,
/// as Booharu's settings and Koharu's Preferred fonts give it.
#[derive(Debug, PartialEq, Serialize)]
struct NewText {
    /// The default font, the first of Koharu's Preferred fonts, which new
    /// text without a family gets; none when they list none.
    font: Option<String>,
    /// The default text colour; none draws Koharu's black.
    fill: Option<Rgba>,
    /// The default outline colour; none for no outline.
    stroke: Option<Rgba>,
    /// The outline's width in pixels; none without an outline.
    stroke_width: Option<f64>,
    /// Whether a new translation keeps the colours Koharu measured, rather
    /// than taking the default ones; the T tool's text always takes them.
    detect_colors: bool,
    /// Pick fonts automatically: whether a new translation is lettered in
    /// its block's lettering category's font rather than `font`; the T
    /// tool's text never is.
    auto: bool,
    /// Which sound effects a run translates; the others keep the original
    /// art.
    sfx: SfxMode,
    /// How a sound effect's subtitle looks, in `font` when its family is
    /// none.
    subtitle: SubtitleStyle,
}

/// The look new text gets from `settings`, `[booharu-lettering]`, and
/// `font`, the default font.
fn new_text((settings, font): (LetteringSettings, Option<String>)) -> NewText {
    let look = settings.default;
    NewText {
        font,
        fill: look.fill,
        stroke: look.stroke,
        stroke_width: look.stroke.map(|_| look.stroke_width),
        detect_colors: settings.detect_colors,
        auto: settings.auto,
        sfx: settings.sfx,
        subtitle: settings.subtitle,
    }
}

/// The translator's settings, and whether its model's vendor is connected.
#[derive(Serialize)]
struct Translator {
    #[serde(flatten)]
    settings: TranslatorConfig,
    /// Whether the vendor has its API key or sign-in stored, as the Providers
    /// settings show it; never the key itself.
    connected: Part<bool>,
}

/// Whether the vendor of `model`, a `vendor/model` reference, is connected,
/// as koharu-desktop's `list_vendors` tells the Providers settings: once
/// `stored`, reading the secret kept under a vendor's name, has its API key or
/// sign-in. Koharu's own models need neither.
fn connected(
    model: &str,
    stored: impl FnOnce(&str) -> koharu_config::Result<Option<SecretString>>,
) -> anyhow::Result<bool> {
    let (vendor, _) = model
        .split_once('/')
        .with_context(|| format!("`{model}` names no model: expected `vendor/model`"))?;
    Ok(vendor == koharu_llm::KOHARU || stored(vendor)?.is_some())
}

#[derive(Serialize)]
struct PageOverview {
    id: PageId,
    size: Size,
    blocks: usize,
    /// The blocks whose translation has some text, so never one with only
    /// the empty placeholder a style set before translating leaves.
    translated: usize,
    /// Booharu's attributes of each layer that holds any, blocks and others.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    attributes: BTreeMap<LayerId, Attrs>,
    /// The parts of each block changed by hand, for the blocks with any.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    hand_edited: BTreeMap<LayerId, Vec<&'static str>>,
}

/// A part of the overview read apart from the project, or why it couldn't
/// be, so one that fails, such as the font list offline, leaves the rest.
#[derive(Serialize)]
#[serde(untagged)]
enum Part<T> {
    Read(T),
    Failed { error: String },
}

impl<T, E: Into<anyhow::Error>> From<Result<T, E>> for Part<T> {
    fn from(read: Result<T, E>) -> Self {
        match read {
            Ok(value) => Self::Read(value),
            Err(error) => Self::Failed {
                error: format!("{:#}", error.into()),
            },
        }
    }
}

/// Each page of `project` in order, with its blocks counted, the Booharu
/// attributes on its layers, and the parts of its blocks changed by hand.
fn pages(project: &Project) -> koharu_scene::Result<Vec<PageOverview>> {
    project
        .pages()
        .map(|page| {
            let mut overview = PageOverview {
                id: page.id,
                size: page.size,
                blocks: 0,
                translated: 0,
                attributes: BTreeMap::new(),
                hand_edited: BTreeMap::new(),
            };
            for layer in project.layers_of(page.id)? {
                if let Content::Block(block) = &layer.content {
                    overview.blocks += 1;
                    let translation = block.translation.as_ref();
                    if translation.is_some_and(|text| !text.content.text().trim().is_empty()) {
                        overview.translated += 1;
                    }
                    let parts = hand_edited(edited(layer));
                    if !parts.is_empty() {
                        overview.hand_edited.insert(layer.id, parts);
                    }
                }
                let attributes = attrs::read(layer);
                if attributes != Attrs::default() {
                    overview.attributes.insert(layer.id, attributes);
                }
            }
            Ok(overview)
        })
        .collect()
}

/// The names of the parts of a block `edited` says were changed by hand.
fn hand_edited(edited: Edited) -> Vec<&'static str> {
    let Edited {
        source,
        text,
        style,
        drawn,
    } = edited;
    [
        ("source", source),
        ("text", text),
        ("style", style),
        ("drawn", drawn),
    ]
    .into_iter()
    .filter_map(|(part, changed)| changed.then_some(part))
    .collect()
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Created, Edit, Session};
    use serde_json::json;

    use super::*;

    /// The overview's translator part for `model`, with `stored` reading the
    /// secrets.
    fn translator(
        model: &str,
        stored: impl FnOnce(&str) -> koharu_config::Result<Option<SecretString>>,
    ) -> Value {
        json!(Translator {
            settings: TranslatorConfig {
                model: model.to_owned(),
                ..TranslatorConfig::default()
            },
            connected: connected(model, stored).into(),
        })
    }

    #[test]
    fn the_translator_says_whether_its_vendor_is_connected_but_never_its_key() {
        let gemini =
            |vendor: &str| Ok((vendor == "gemini").then(|| SecretString::from("sk-gemini")));
        let read = translator("gemini/gemini-2.5-pro", gemini);
        assert_eq!(read["model"], json!("gemini/gemini-2.5-pro"));
        assert_eq!(read["connected"], json!(true));
        assert!(!read.to_string().contains("sk-gemini"), "{read}");
        assert_eq!(
            translator("openai/gpt-5", gemini)["connected"],
            json!(false)
        );

        // Koharu's own models need no key, so a store that can't answer
        // isn't asked.
        let locked = |_: &str| Err(koharu_config::Error::NoHome);
        let local = translator("koharu/index-translate-9b", locked);
        assert_eq!(local["connected"], json!(true));
        let unknown = translator("openai/gpt-5", locked);
        assert!(unknown["connected"]["error"].is_string(), "{unknown}");
    }

    #[test]
    fn the_overview_counts_a_pages_blocks_and_lists_their_booharu_attributes() {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let add = |label: &str, layer: Value| json!({"op": "add_layer", "id": format!("${label}"), "page": "$page", "index": 0, "layer": layer});
        let set = |label: &str, namespace: &str, key: &str, value: Value| json!({"op": "set_attribute", "layer": format!("${label}"), "namespace": namespace, "key": key, "value": value});
        let lettering = json!({"category": "shout", "source": "user"});
        let ops = [
            json!({"op": "add_page", "id": "$page", "index": 0, "size": {"width": 100, "height": 200}}),
            add(
                "plain",
                json!({"kind": "block", "source": {"content": "やあ"}}),
            ),
            add(
                "done",
                json!({"kind": "block", "source": {"content": "やあ"}, "translation": {"content": "Hi"}}),
            ),
            add(
                "blank",
                json!({"kind": "block", "source": {"content": "やあ"}, "translation": {"content": " "}}),
            ),
            add("note", json!({"kind": "text", "content": "a"})),
            set("plain", "booharu", "lettering", lettering.clone()),
            set("plain", "c2pa", "ocr", json!({"action": "c2pa.created"})),
            set(
                "note",
                "booharu",
                "skew",
                json!({"x": 10.0, "y": 0.0, "pivot": [0.0, 0.0]}),
            ),
            set("done", "booharu", "skew", json!("not a skew")),
        ];
        let ops = ops
            .into_iter()
            .map(|op| serde_json::from_value(op).expect("an op"));
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops: ops.collect(),
            })
            .expect("a page");
        let id = |label: &str| {
            let id = change.created.iter().find_map(|created| match created {
                Created::Layer {
                    id,
                    label: Some(found),
                } if found == label => Some(id.to_string()),
                _ => None,
            });
            id.expect("a layer")
        };
        let project = session.project();
        let page = project.pages().next().expect("the page").id;

        let overview = pages(&project).expect("the pages");

        // The c2pa attribute is another namespace's, and the bad skew reads
        // as none. No stage recorded detecting the blocks, nor reading or
        // translating the translated ones, so those parts count as made by
        // hand, but for the blank text: unplaced and written by no run, it's
        // the placeholder a style set before translating leaves, no words
        // typed.
        let everything = json!(["source", "text", "style", "drawn"]);
        assert_eq!(
            json!(overview),
            json!([{
                "id": page,
                "size": {"width": 100, "height": 200},
                "blocks": 3,
                "translated": 1,
                "attributes": {
                    id("plain"): {"lettering": lettering},
                    id("note"): {"skew": {"x": 10.0, "y": 0.0, "pivot": [0.0, 0.0]}},
                },
                "hand_edited": {
                    id("plain"): ["drawn"],
                    id("done"): everything,
                    id("blank"): ["source", "style", "drawn"],
                },
            }])
        );
    }

    #[test]
    fn the_overview_names_only_the_parts_changed_by_hand_of_each_block() {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        // Found, read, and translated by Koharu's stages.
        let staged = json!({
            "detection": {"action": "c2pa.created"},
            "ocr": {"action": "c2pa.created"},
            "translation": {"action": "c2pa.translated"},
        });
        let block = |label: &str, c2pa: Value, booharu: Value| {
            json!({"op": "add_layer", "id": format!("${label}"), "page": "$page", "index": 0, "layer": {
                "kind": "block",
                "source": {"content": "やあ"},
                "translation": {"content": "Hi"},
                "attributes": {"c2pa": c2pa, "booharu": booharu},
            }})
        };
        let typed =
            json!({"detection": {"action": "c2pa.created"}, "ocr": {"action": "c2pa.created"}});
        let ops = [
            json!({"op": "add_page", "id": "$page", "index": 0, "size": {"width": 100, "height": 200}}),
            block("auto", staged.clone(), json!({})),
            block("typed", typed, json!({})),
            block("agent", staged, json!({"auto": {"origin": "agent"}})),
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
        let id = |label: &str| {
            let id = change.created.iter().find_map(|created| match created {
                Created::Layer {
                    id,
                    label: Some(found),
                } if found == label => Some(id.to_string()),
                _ => None,
            });
            id.expect("a layer")
        };

        let overview = pages(&session.project()).expect("the pages");

        // The agent's chosen look counts as a hand edit of the style alone.
        assert_eq!(
            json!(overview[0].hand_edited),
            json!({id("typed"): ["text", "style"], id("agent"): ["style"]})
        );
    }

    #[test]
    fn a_block_styled_before_its_translated_counts_untranslated_with_its_style_set_by_hand() {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let mut apply = |ops: Vec<Value>| {
            session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops: ops
                        .into_iter()
                        .map(|op| serde_json::from_value(op).expect("an op"))
                        .collect(),
                })
                .expect("an edit")
        };
        // Found and read by Koharu's stages, never translated.
        let change = apply(vec![
            json!({"op": "add_page", "id": "$page", "index": 0, "size": {"width": 100, "height": 200}}),
            json!({"op": "add_layer", "id": "$styled", "page": "$page", "index": 0, "layer": {
                "kind": "block",
                "source": {"content": "やあ"},
                "attributes": {"c2pa": {
                    "detection": {"action": "c2pa.created"},
                    "ocr": {"action": "c2pa.created"},
                }},
            }}),
        ]);
        let styled = change
            .created
            .iter()
            .find_map(|created| match created {
                Created::Layer {
                    id,
                    label: Some(label),
                } if label == "styled" => Some(id.to_string()),
                _ => None,
            })
            .expect("the block");
        // A font and a fill picked in the type bar, merged into a translation
        // the block doesn't have, start one from empty point text.
        apply(vec![json!({
            "op": "update_layer",
            "layer": styled,
            "patch": {"translation": {"style": {"chars": {
                "family": "Comic Neue",
                "fill": [0.0, 0.5, 0.0, 1.0],
            }}}},
        })]);

        let overview = pages(&session.project()).expect("the pages");

        // Its empty placeholder is no translation, nor one typed by hand,
        // but the look picked for it is the user's.
        assert_eq!((overview[0].blocks, overview[0].translated), (1, 0));
        assert_eq!(json!(overview[0].hand_edited), json!({styled: ["style"]}));
    }

    #[test]
    fn the_overview_tells_the_look_new_text_gets() {
        let red = Rgba([1.0, 0.0, 0.0, 1.0]);
        let mut settings = LetteringSettings::default();
        assert_eq!(
            new_text((settings.clone(), Some("Anime Ace".to_owned()))),
            NewText {
                font: Some("Anime Ace".to_owned()),
                fill: None,
                stroke: None,
                stroke_width: None,
                detect_colors: true,
                auto: false,
                sfx: SfxMode::Skip,
                subtitle: SubtitleStyle::default(),
            }
        );
        settings.default.fill = Some(red);
        settings.default.stroke = Some(Rgba([1.0; 4]));
        settings.default.stroke_width = 3.0;
        settings.detect_colors = false;
        settings.auto = true;
        settings.sfx = SfxMode::InsideBubbles;
        settings.subtitle.family = Some("Wild Words".to_owned());
        settings.subtitle.fill = red;
        assert_eq!(
            json!(new_text((settings, None))),
            json!({
                "font": null,
                "fill": [1.0, 0.0, 0.0, 1.0],
                "stroke": [1.0, 1.0, 1.0, 1.0],
                "stroke_width": 3.0,
                "detect_colors": false,
                "auto": true,
                "sfx": "inside_bubbles",
                "subtitle": {
                    "family": "Wild Words",
                    "size": 0.015,
                    "fill": [1.0, 0.0, 0.0, 1.0],
                    "stroke": [1.0, 1.0, 1.0, 1.0],
                },
            })
        );
    }
}
