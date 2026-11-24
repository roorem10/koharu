//! Read chapter as the agent starts it, and the chapter brief it writes as
//! the agent edits it: `booharu_read_chapter` reads the whole chapter
//! through the app's host, which reads it as the Context tab's button does,
//! and waits for the read to end; `booharu_set_chapter_brief` saves the
//! brief's summary as the user's, as the brief's View dialog saves it, with
//! `booharu::context::set_brief`. The runs the agent and outside apps start
//! never read the chapter on their own: the chapter in short that goes with
//! each of the agent's requests says to read one with text and no brief
//! first (`booharu::context::NO_BRIEF`).

use std::sync::Arc;

use booharu::context::{self, read::ReadSummary};
use booharu_core::context::{ChapterBrief, ChapterContext};
use rig_core::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::context::{blocking, file, host};
use crate::tools::{failed, project, schema};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadChapterArgs {
    /// The model to analyze with in place of the Analyze chapter model, as
    /// a `vendor/model` reference, for this analysis alone; it isn't saved.
    model: Option<String>,
}

pub(super) struct ReadChapter;

impl Tool for ReadChapter {
    const NAME: &'static str = "booharu_read_chapter";
    type Args = ReadChapterArgs;
    type Output = ReadSummary;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Analyze chapter, as the Context tab's button does once the user says yes, but at once, \
         asking nothing: a model that sees the pages analyzes the open chapter whole and, once \
         every page is analyzed, writes the chapter's brief, what happens and who is in it, \
         keeping a summary the user wrote; suggests the terms it finds that the linked series \
         and the suggestions lack; and gives each line its lettering category, never over a \
         category the user chose, and a note, leaving locked lines alone. It analyzes with the \
         Analyze chapter model, else with the translation model when that one can, or with \
         `model` for this analysis alone, which isn't saved; a model that analyzes must see \
         images and call tools. A run you start doesn't analyze the chapter first on its own: \
         call this before translating a chapter that has text but no brief, as the `booharu` \
         part of the project's outline then says. An analysis takes a while, and a hosted model \
         charges for it. Answers once the analysis ends: what it did, with the `model` that \
         analyzed and how many `pages` it analyzed in how many `chunks`, or `stopped` when Stop \
         or closing the project ended it, having written nothing. Refused while a run, a Run \
         press or another analysis goes, or while Koharu's Gallery is still loading pages into \
         the chapter, saying to wait until they're in; with no text on the pages, saying to run \
         OCR first; and with no model that can analyze them, saying why: tell the user, who can \
         choose an Analyze chapter model in Settings > Pipeline. A `model` you name that can't \
         analyze is refused by name: call again without it, or with one that can. Only in the \
         app. Needs a project open, as every tool here does."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<ReadChapterArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: ReadChapterArgs,
    ) -> Result<ReadSummary, Self::Error> {
        let tools = project(context)?;
        // Awaited here, so a run stopped while it reads stops the read.
        host(&tools)
            .read_chapter(Arc::clone(&tools.session), args.model)
            .await
            .map_err(|error| failed(format!("{error:#}")))
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SetChapterBriefArgs {
    /// What happens in the chapter, in a few sentences; blank for none.
    summary: String,
}

/// What saving the brief's summary did.
#[derive(Debug, Serialize)]
pub(super) struct SavedBrief {
    /// Whether it changed anything: false when the brief had that summary,
    /// as the user's, already.
    changed: bool,
    /// The brief as saved.
    brief: Option<ChapterBrief>,
}

pub(super) struct SetChapterBrief;

impl Tool for SetChapterBrief {
    const NAME: &'static str = "booharu_set_chapter_brief";
    type Args = SetChapterBriefArgs;
    type Output = SavedBrief;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Set the summary of the open chapter's brief, what happens in it, as Save brief in the \
         Context tab's brief dialog does: trimmed, and marked as the user's, so analyzing the \
         chapter again keeps it; the brief's characters, and when and with which model the \
         chapter was analyzed, stay as they are. Every translation run on the chapter sends the \
         summary. A blank summary counts as no brief, so the next analysis writes one. Answers \
         whether it `changed` anything, and the `brief` as saved. Needs a project open, as \
         every tool here does."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<SetChapterBriefArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: SetChapterBriefArgs,
    ) -> Result<SavedBrief, Self::Error> {
        blocking(context, move |tools, host| {
            let path = file(tools);
            let changed = context::set_brief(&path, &args.summary)?;
            if changed {
                host.context_changed();
            }
            Ok(SavedBrief {
                changed,
                brief: ChapterContext::load(&path)?.brief,
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use booharu_core::{context::ChapterCharacter, settings::BooharuPreferences};
    use koharu_pipeline::{Pipeline, PipelineConfig};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::{Edit, Session};
    use parking_lot::Mutex;
    use serde_json::{Map, json};

    use super::*;
    use crate::tools::{Tools, booharu::BooharuHost};

    /// The app as the chapter tools reach it, but for Read chapter: how
    /// many writes it was told of.
    #[derive(Default)]
    struct App {
        told: AtomicUsize,
    }

    impl BooharuHost for App {
        fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
            anyhow::bail!("nothing here reads settings")
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

        fn context_changed(&self) {
            self.told.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// A chapter of one page, in a folder of the test's, holding a block
    /// found and read as `source` when there's one, and its tools, with
    /// `app` when there's one.
    fn chapter(source: Option<&str>, app: Option<Arc<App>>) -> (tempfile::TempDir, Tools) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("Ch.1.khr")).expect("a project");
        let page = json!({"op": "add_page", "id": "$page", "index": 0,
            "size": {"width": 400, "height": 400}});
        let block = source.map(|source| {
            json!({"op": "add_layer", "id": "$block", "page": "$page", "index": 0,
            "layer": {"kind": "block", "source": {
                "frame": {"box": {"bounds": {"x": 10, "y": 10, "width": 30, "height": 100}}},
                "content": source,
            }, "attributes": {"c2pa": {
                "detection": {"action": "c2pa.created"},
                "ocr": {"action": "c2pa.created"},
            }}}})
        });
        let ops = std::iter::once(page)
            .chain(block)
            .map(|op| serde_json::from_value(op).expect("an op"))
            .collect();
        session
            .apply(Edit {
                label: "test".to_owned(),
                ops,
            })
            .expect("the page");
        let tools = Tools::new(
            Arc::new(Mutex::new(session)),
            Pipeline::new(PipelineConfig::default()),
            Renderer::new(RendererConfig::default()),
        );
        let tools = match app {
            Some(app) => tools.with_booharu(app),
            None => tools,
        };
        (dir, tools)
    }

    async fn call<T: Tool>(tools: &Tools, tool: T, args: Value) -> Result<T::Output, T::Error> {
        let args = serde_json::from_value(args).expect("the arguments");
        tool.call(&mut tools.tool_context(), args).await
    }

    /// The brief a read wrote at 10:00 with `openai/gpt-5.4`.
    fn read() -> ChapterBrief {
        ChapterBrief {
            summary: "Hina meets Sensei.".to_owned(),
            characters: vec![ChapterCharacter {
                name: "Hina".to_owned(),
                original: "ヒナ".to_owned(),
                pages: vec![1],
            }],
            edited: false,
            read_at: Some("2026-11-19T10:00:00Z".to_owned()),
            read_model: Some("openai/gpt-5.4".to_owned()),
        }
    }

    #[tokio::test]
    async fn read_chapter_is_refused_outside_the_app() {
        for app in [None, Some(Arc::new(App::default()))] {
            let (_dir, tools) = chapter(Some("ヒナ先生!"), app);
            let error = call(&tools, ReadChapter, json!({"model": "openai/gpt-5.4"}))
                .await
                .expect_err("no app to read with");
            assert!(error.to_string().contains("runs in the app"), "{error}");
        }
    }

    #[tokio::test]
    async fn the_brief_set_keeps_its_characters_and_is_the_users() {
        let app = Arc::new(App::default());
        let (_dir, tools) = chapter(Some("ヒナ先生!"), Some(Arc::clone(&app)));
        let path = file(&tools);
        ChapterContext::update(&path, |chapter| chapter.brief = Some(read())).expect("read");
        let args = json!({"summary": "  Hina joins the Prefect Team.  "});
        let saved = call(&tools, SetChapterBrief, args.clone())
            .await
            .expect("saved");
        let brief = ChapterBrief {
            summary: "Hina joins the Prefect Team.".to_owned(),
            edited: true,
            ..read()
        };
        assert!(saved.changed);
        assert_eq!(saved.brief.as_ref(), Some(&brief));
        let chapter = ChapterContext::load(&path).expect("the chapter");
        assert_eq!(chapter.brief, Some(brief));
        assert_eq!(app.told.load(Ordering::Relaxed), 1);
        let again = call(&tools, SetChapterBrief, args).await.expect("saved");
        assert!(!again.changed, "{again:?}");
        assert_eq!(app.told.load(Ordering::Relaxed), 1, "nothing written");
    }

    #[tokio::test]
    async fn the_context_document_says_to_read_a_chapter_with_text_and_no_brief() {
        let (_dir, blank) = chapter(None, None);
        let outline = blank.context().expect("the outline");
        assert_eq!(
            outline["booharu"],
            json!({"series": null}),
            "no text to read"
        );
        let (_dir, tools) = chapter(Some("ヒナ先生!"), None);
        let outline = tools.context().expect("the outline");
        assert_eq!(outline["booharu"]["brief"], json!(context::NO_BRIEF));
        // A blank summary is no brief.
        context::set_brief(&file(&tools), " ").expect("saved");
        let outline = tools.context().expect("the outline");
        assert_eq!(outline["booharu"]["brief"], json!(context::NO_BRIEF));
        ChapterContext::update(&file(&tools), |chapter| chapter.brief = Some(read()))
            .expect("read");
        let outline = tools.context().expect("the outline");
        assert_eq!(
            outline["booharu"]["brief"],
            json!({"summary": "Hina meets Sensei.",
                "characters": [{"name": "Hina", "original": "ヒナ", "pages": [1]}],
                "edited": false, "read_at": "2026-11-19T10:00:00Z", "read_model": "openai/gpt-5.4"})
        );
    }

    /// Every string in `value`, its keys too, after `into`.
    fn strings(value: &Value, into: &mut String) {
        match value {
            Value::String(text) => {
                into.push(' ');
                into.push_str(text);
            }
            Value::Array(items) => items.iter().for_each(|item| strings(item, into)),
            Value::Object(fields) => fields.iter().for_each(|(key, item)| {
                into.push(' ');
                into.push_str(key);
                strings(item, into);
            }),
            _ => {}
        }
    }

    /// The agent and MCP clients are offered Analyze chapter by that name,
    /// never by Read chapter, its name until Koharu 0.94.0 brought its own
    /// Read mode: in each tool's description, Koharu's tools' Booharu
    /// sentences among them, and in its parameters, whose descriptions
    /// schemars takes from the doc comments of the types they read, as
    /// `booharu_run`'s `read` stage's from `RunStage::Read`.
    #[test]
    fn the_tools_offer_analyze_chapter_by_its_name() {
        let definitions = Tools::toolset().tool_definitions();
        let offered = |name: &str| {
            let definition = definitions
                .iter()
                .find(|definition| definition.name.as_str() == name)
                .unwrap_or_else(|| panic!("no {name}"));
            let mut words = definition.description.clone();
            strings(&definition.parameters, &mut words);
            words.split_whitespace().collect::<Vec<_>>().join(" ")
        };
        for definition in &definitions {
            let name = definition.name.as_str();
            let words = offered(name);
            assert!(!words.contains("Read chapter"), "{name}: {words}");
        }
        let run = offered("booharu_run");
        assert!(run.contains("`read` (Analyze chapter, only together with `translate`)"));
        assert!(
            run.contains("Analyze chapter: the whole chapter analyzed before it is translated")
        );
        let read = offered("booharu_read_chapter");
        assert!(
            read.contains("in place of the Analyze chapter model"),
            "{read}"
        );
    }
}
