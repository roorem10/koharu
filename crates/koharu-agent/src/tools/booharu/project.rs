//! The project overview: what an agent needs to work on the open project, in
//! one read, beside `get_project`'s pages and `get_page`'s layers.

use std::collections::BTreeMap;

use anyhow::Context as _;
use booharu_core::{
    attrs::{self, Attrs},
    settings::BooharuPreferences,
};
use koharu_config::secrets::SecretString;
use koharu_pipeline::{PipelineConfig, TranslatorConfig};
use koharu_renderer::{RendererConfig, fonts};
use koharu_scene::{Content, LayerId, PageId, Project, Size};
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
         size, how many blocks it has and how many of them have a translation, and Booharu's \
         attributes of each of its layers that has any, by layer id; the translator's model, \
         target language, and instructions, with `connected`: whether the model's vendor has its \
         API key or sign-in stored, as the Providers settings show it, never the key itself \
         (Koharu's own models need neither, nor may a server named by its base URL); the \
         typesetting defaults, such as the font families a text falls back on; the name of every \
         font family a text can be set in; and Booharu's settings. A part that can't be read \
         gives its error instead."
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
    /// Booharu's settings sections by table name, from the app; none outside
    /// it, as in the `serve` example.
    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<Part<BooharuPreferences>>,
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
    /// The blocks whose translation has some text.
    translated: usize,
    /// Booharu's attributes of each layer that holds any, blocks and others.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    attributes: BTreeMap<LayerId, Attrs>,
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

/// Each page of `project` in order, with its blocks counted and the
/// Booharu attributes on its layers.
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
            };
            for layer in project.layers_of(page.id)? {
                if let Content::Block(block) = &layer.content {
                    overview.blocks += 1;
                    let translation = block.translation.as_ref();
                    if translation.is_some_and(|text| !text.content.text().trim().is_empty()) {
                        overview.translated += 1;
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
            set("note", "booharu", "skew", json!({"x": 10.0, "y": 0.0})),
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
        // as none.
        assert_eq!(
            json!(overview),
            json!([{
                "id": page,
                "size": {"width": 100, "height": 200},
                "blocks": 3,
                "translated": 1,
                "attributes": {
                    id("plain"): {"lettering": lettering},
                    id("note"): {"skew": {"x": 10.0, "y": 0.0}},
                },
            }])
        );
    }
}
