//! The quality check, as the agent works with it: what on the pages needs a
//! look, worked out on demand by `booharu::qc` as the app's QC tab finds it,
//! with the same settings, glossary and rules, since the agent can't reach
//! the app's findings; the glossary slips alone; and Mark OK, the agent's
//! flags on lines, and dismissing them, each one undo step built by
//! `booharu::qc`'s edits, as the app's commands build them. The app's
//! quality check follows these edits by itself, as it follows every change
//! to the project.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use booharu::qc::{self, Finding, Flagged, Inputs, Problem};
use koharu_scene::{Change, Edit, LayerId, PageId, Project};
use rig_core::{
    completion::ToolAnnotations,
    tool::{Tool, ToolContext, ToolExecutionError},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::{BooharuHost, context::blocking};
use crate::tools::{Tools, failed, project, schema};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct PagesArgs {
    /// The page to check; every page when left out.
    page: Option<PageId>,
}

/// A finding, with the number of the page it's on.
#[derive(Serialize)]
pub(super) struct PageFinding {
    /// The page's number, counted from 1 in page order, as the user sees
    /// it.
    page_number: usize,
    #[serde(flatten)]
    finding: Finding,
}

pub(super) struct GetQcFindings;

impl Tool for GetQcFindings {
    const NAME: &'static str = "booharu_get_qc_findings";
    type Args = PagesArgs;
    type Output = Vec<PageFinding>;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "What the quality check finds on the open project's pages, as the QC tab shows it, \
         worked out now: on `page`, or on every page when it's left out, page by page, each \
         page's lines in reading order. Each finding names its page by `page_number`, counted \
         from 1 in page order, and by id (`page`), the block or text layer it's on (`layer`), \
         its `check`, the `problem` with its details, who found it (`source`: `auto` for \
         Booharu's own checks, `agent` for a line flagged for review with booharu_flag_line), \
         the line's `text` (a block's translation, else its source, or a text layer's text), \
         and whether it's `hidden` because the user marked it OK while the line is as it is \
         now. The checks: `overflow`, text running past its box; `too_small`, text drawn under \
         the smallest readable size, `min_size` in `booharu-qc`, a share of the page's shorter \
         side; `small_word`, one styled word drawn that small while the rest isn't; \
         `missing_translation`, a line with source text but no translation, its lettering \
         cleaned away, on a page Koharu's Translation stage ran on; `empty_source`, a block \
         found and read with nothing in it; `leftover_source`, source text in a script the \
         translation's language doesn't write, or, `unchanged`, a line the model gave back \
         untranslated, its source lettering showing; `odd_font`, a font other than the one \
         most blocks of its lettering category are set in across the chapter, never for a font \
         set by hand; `glossary:` and a term's source, a translation leaving out a term of the \
         linked series' glossary (booharu_check_glossary lists those alone); and `flag`, a \
         line flagged for review. Locked layers, and layers in locked ones, count as finished \
         and are never checked. Sizes are page pixels. Measuring every line takes a while on a \
         long chapter, so name a page when you can."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<PagesArgs>()
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations::default().with_read_only(true)
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: PagesArgs,
    ) -> Result<Vec<PageFinding>, Self::Error> {
        // A call dropped, as when the agent is stopped, lays out no more
        // pages.
        let dropped = CancellationToken::new();
        let _stops = dropped.clone().drop_guard();
        findings(context, args.page, move || dropped.is_cancelled()).await
    }
}

/// The findings on `page`, or on every page, of the open project, as
/// booharu_get_qc_findings answers them: the pages are laid out one at a
/// time, `stopped` asked before each, and none once it says to stop. What
/// the check reads is taken with the tools, which are let go before any page
/// is laid out: they hold the session, which keeps its project's file locked
/// while it's held, so a project closed meanwhile would open again only once
/// every page was laid out.
async fn findings(
    context: &ToolContext,
    page: Option<PageId>,
    stopped: impl Fn() -> bool + Send + 'static,
) -> Result<Vec<PageFinding>, ToolExecutionError> {
    let (renderer, project, inputs) = blocking(context, |tools, host| {
        let (file, project) = opened(tools);
        let inputs = inputs(host, &file, &project)?;
        Ok((tools.renderer.clone(), project, inputs))
    })
    .await?;
    tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<PageFinding>> {
        let mut found = Vec::new();
        for (page_number, page) in numbered(&project, page)? {
            if stopped() {
                break;
            }
            let findings = qc::check_page(&renderer, &project, page, &inputs)?;
            found.extend(findings.into_iter().map(|finding| PageFinding {
                page_number,
                finding,
            }));
        }
        Ok(found)
    })
    .await
    .map_err(failed)?
    .map_err(|error| failed(format!("{error:#}")))
}

/// A glossary slip: a block whose translation leaves out a term of the
/// linked series' glossary.
#[derive(Serialize)]
pub(super) struct Slip {
    /// The page's number, counted from 1 in page order, as the user sees
    /// it.
    page_number: usize,
    page: PageId,
    /// The block.
    layer: LayerId,
    /// The check `booharu_mark_qc_ok` takes: `glossary:` and the term's
    /// source.
    check: String,
    /// The term's source, which the block's source text uses.
    source: String,
    /// What the term is to be translated as.
    target: String,
    /// The block's translation, which leaves the term out.
    translation: String,
    /// Whether the user marked it OK while the block is as it is now.
    hidden: bool,
}

pub(super) struct CheckGlossary;

impl Tool for CheckGlossary {
    const NAME: &'static str = "booharu_check_glossary";
    type Args = PagesArgs;
    type Output = Vec<Slip>;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "The glossary slips on the open project's pages, as the quality check finds them, \
         without measuring any text: on `page`, or on every page when it's left out, page by \
         page, each page's lines in reading order. A slip is a block whose translation leaves \
         out a term of the glossary of the series the chapter is linked to: the term's \
         `source` is in the block's source text, but its `target` isn't in the block's \
         `translation`, ignoring case and spaces. Each names its page by `page_number`, \
         counted from 1 in page order, and by id (`page`), the block (`layer`), the `check` \
         booharu_mark_qc_ok takes, and whether it's `hidden` because the user marked it OK \
         while the block is as it is now. Only the series' own terms are checked, not the \
         ones suggested for the chapter; locked blocks, blocks in locked layers, and blank \
         translations or ones that are their source are never checked; with no series linked, \
         or one that can't be read, there are none. booharu_set_translation fixes one line; \
         booharu_fix_term_everywhere translates every line using a term again."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<PagesArgs>()
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations::default().with_read_only(true)
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: PagesArgs,
    ) -> Result<Vec<Slip>, Self::Error> {
        blocking(context, move |tools, host| {
            let (file, project) = opened(tools);
            let inputs = inputs(host, &file, &project)?;
            let mut slips = Vec::new();
            for (page_number, page) in numbered(&project, args.page)? {
                for finding in qc::check_glossary(&project, page, &inputs)? {
                    let Problem::Glossary { source, target } = finding.problem else {
                        continue;
                    };
                    slips.push(Slip {
                        page_number,
                        page,
                        layer: finding.layer,
                        check: finding.check,
                        source,
                        target,
                        translation: finding.text,
                        hidden: finding.hidden,
                    });
                }
            }
            Ok(slips)
        })
        .await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct MarkQcOkArgs {
    /// The finding's block or text layer.
    layer: LayerId,
    /// The finding's check, as booharu_get_qc_findings answers it.
    check: String,
    /// Whether to mark it OK; false takes the mark back. Left out, it's
    /// marked OK.
    #[serde(default = "marked")]
    ok: bool,
}

/// Mark OK's default: the finding is marked OK.
fn marked() -> bool {
    true
}

pub(super) struct MarkQcOk;

impl Tool for MarkQcOk {
    const NAME: &'static str = "booharu_mark_qc_ok";
    type Args = MarkQcOkArgs;
    type Output = Option<Change>;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Mark a finding of the quality check OK, as Mark OK in the QC tab does, so it's hidden \
         while its line stays as it is now and comes back once the line changes, or, with `ok` \
         false, take the mark back, as Undo OK does; as one undo step. `layer` is the \
         finding's block or text layer and `check` its check, as booharu_get_qc_findings \
         answers them: `overflow`, `too_small`, `small_word`, `missing_translation`, \
         `empty_source`, `leftover_source`, `odd_font`, or `glossary:` and the term's source. \
         Use it for a finding that's fine as it is, such as a term the line words differently \
         on purpose; fix the others. A flag isn't marked OK but dismissed \
         (booharu_dismiss_flag). A locked layer, or one inside a locked layer, refuses it. \
         Answers what changed, or null when the marks stay as they were."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<MarkQcOkArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: MarkQcOkArgs,
    ) -> Result<Option<Change>, Self::Error> {
        let tools = project(context)?;
        let MarkQcOkArgs { layer, check, ok } = args;
        let (change, ()) = apply_if(&tools, move |project| {
            Ok((qc::mark_qc_ok(project, layer, &check, ok)?, ()))
        })
        .await?;
        Ok(change)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct FlagLineArgs {
    /// The lines to flag.
    lines: Vec<LineFlag>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct LineFlag {
    /// The block or text layer.
    layer: LayerId,
    /// What a person should look at, in a few words: 1 to 500 characters.
    message: String,
}

pub(super) struct FlagLine;

impl Tool for FlagLine {
    const NAME: &'static str = "booharu_flag_line";
    type Args = FlagLineArgs;
    type Output = Flagged;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Mark lines for human review, each with a short message, as one undo step: each of \
         `lines` is a block or a text layer and what a person should look at, such as a pun \
         you couldn't keep or a name you aren't sure of. A flag shows in the QC tab as the \
         agent's until someone dismisses it (booharu_dismiss_flag); Mark OK doesn't hide it. A \
         locked line, or one inside a locked layer, is left out (`locked`), as is a line that \
         already has a flag with that message (`duplicates`). A message blank or over 500 \
         characters once trimmed, or a layer that is neither a block nor a text, refuses the \
         whole call. Answers the lines `flagged`, `duplicates`, and `locked`."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<FlagLineArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: FlagLineArgs,
    ) -> Result<Flagged, Self::Error> {
        let tools = project(context)?;
        let lines: Vec<(LayerId, String)> = args
            .lines
            .into_iter()
            .map(|line| (line.layer, line.message))
            .collect();
        let (_, flagged) = apply_if(&tools, move |project| qc::flag_line(project, &lines)).await?;
        Ok(flagged)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct DismissFlagArgs {
    /// The block or text layer.
    layer: LayerId,
    /// The message of the flag to dismiss; every flag on the layer when
    /// left out.
    message: Option<String>,
}

/// What dismissing flags did.
#[derive(Debug, Serialize)]
pub(super) struct Dismissed {
    /// How many flags it dismissed.
    dismissed: usize,
}

pub(super) struct DismissFlag;

impl Tool for DismissFlag {
    const NAME: &'static str = "booharu_dismiss_flag";
    type Args = DismissFlagArgs;
    type Output = Dismissed;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Dismiss flags on a block or text layer, as Dismiss in the QC tab does, as one undo \
         step: the ones whose message is `message`, ignoring spaces around it, or every flag \
         on the layer when it's left out. A locked layer, or one inside a locked layer, \
         refuses it. Answers how many it `dismissed`; 0 changes nothing."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<DismissFlagArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: DismissFlagArgs,
    ) -> Result<Dismissed, Self::Error> {
        let tools = project(context)?;
        let DismissFlagArgs { layer, message } = args;
        let (_, dismissed) = apply_if(&tools, move |project| {
            qc::dismiss_flag(project, layer, message.as_deref())
        })
        .await?;
        Ok(Dismissed { dismissed })
    }
}

/// The open project's file and the project as it is now, read under one
/// lock.
fn opened(tools: &Tools) -> (PathBuf, Project) {
    let session = tools.session.lock();
    (session.path().to_owned(), session.project())
}

/// What a check of `project`, which the project file `file` holds, reads,
/// as the app's quality check reads it: the three settings sections it
/// reads, as the app holds them, and the series the chapter links, from
/// the app's series folder. One of those sections that can't be read, as
/// when the file isn't TOML, is an error, which reaches the model, and no
/// other section is read; a chapter or series that can't be read skips the
/// glossary check alone.
fn inputs(host: &dyn BooharuHost, file: &Path, project: &Project) -> anyhow::Result<Inputs> {
    Ok(qc::inputs(
        project,
        file,
        || host.series(),
        host.qc_settings()?,
    ))
}

/// The pages to check, each with its number, counted from 1 in page order:
/// `page` alone, or every page when none. A page the project doesn't have
/// fails.
fn numbered(project: &Project, page: Option<PageId>) -> anyhow::Result<Vec<(usize, PageId)>> {
    if let Some(page) = page {
        project.page(page)?;
    }
    Ok(project
        .pages()
        .enumerate()
        .map(|(index, view)| (index + 1, view.id))
        .filter(|(_, id)| page.is_none_or(|page| *id == page))
        .collect())
}

/// Builds with `build`, from the open project, the edit a change needs, if
/// any, and what to answer, and applies the edit as one undo step under the
/// lock it was built under, so nothing changes in between, on a blocking
/// thread, as `apply` does; no edit makes no undo step. A refusal reaches
/// the model word for word.
async fn apply_if<T: Send + 'static>(
    tools: &Arc<Tools>,
    build: impl FnOnce(&Project) -> anyhow::Result<(Option<Edit>, T)> + Send + 'static,
) -> Result<(Option<Change>, T), ToolExecutionError> {
    let tools = Arc::clone(tools);
    tokio::task::spawn_blocking(move || -> anyhow::Result<(Option<Change>, T)> {
        let mut session = tools.session.lock();
        let (edit, answer) = build(&session.project())?;
        let change = edit.map(|edit| session.apply(edit)).transpose()?;
        Ok((change, answer))
    })
    .await
    .map_err(failed)?
    .map_err(|error| failed(format!("{error:#}")))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
    };

    use booharu_core::{
        context::{GlossaryEntry, SeriesStore},
        settings::{BooharuPreferences, LetteringSettings, QcSettings},
    };
    use koharu_pipeline::{Pipeline, PipelineConfig};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::{Created, Session};
    use parking_lot::Mutex;
    use serde_json::{Map, json};

    use super::*;

    /// Every quality check tool, and whether it only reads.
    const TOOLS: [(&str, bool); 5] = [
        ("booharu_get_qc_findings", true),
        ("booharu_check_glossary", true),
        ("booharu_mark_qc_ok", false),
        ("booharu_flag_line", false),
        ("booharu_dismiss_flag", false),
    ];

    /// The app as the quality check tools reach it: the quality check's and
    /// the lettering's default settings, English the target, the whole of
    /// Booharu's settings failing to read, which the check never asks for,
    /// and its series in a folder of the test's.
    struct Checked {
        series: SeriesStore,
    }

    impl BooharuHost for Checked {
        fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
            anyhow::bail!("config.toml is invalid: it isn't UTF-8")
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

        fn series(&self) -> anyhow::Result<SeriesStore> {
            Ok(self.series.clone())
        }

        fn qc_settings(&self) -> anyhow::Result<qc::Settings> {
            Ok(qc::Settings {
                qc: QcSettings::default(),
                target: "en".parse()?,
                lettering: LetteringSettings::default(),
            })
        }
    }

    /// A chapter of two pages, the blocks the ops add on the second, in
    /// the app [`Checked`] stands for, with its series in the chapter's
    /// folder: the tools on it, the second page, and the blocks by name.
    struct Chapter {
        _dir: tempfile::TempDir,
        tools: Tools,
        series: SeriesStore,
        page: PageId,
        layers: BTreeMap<String, LayerId>,
    }

    impl Chapter {
        fn new(ops: Vec<Value>) -> Self {
            let dir = tempfile::tempdir().expect("a folder");
            let mut session = Session::create(dir.path().join("Ch.1.khr")).expect("a project");
            let pages = [
                json!({"op": "add_page", "id": "$first", "index": 0,
                    "size": {"width": 400, "height": 400}}),
                json!({"op": "add_page", "id": "$page", "index": 1,
                    "size": {"width": 400, "height": 400}}),
            ];
            let change = session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops: pages
                        .into_iter()
                        .chain(ops)
                        .map(|op| serde_json::from_value(op).expect("an op"))
                        .collect(),
                })
                .expect("the pages");
            let mut page = None;
            let mut layers = BTreeMap::new();
            for created in change.created {
                match created {
                    Created::Page {
                        id,
                        label: Some(label),
                    } if label == "page" => page = Some(id),
                    Created::Layer {
                        id,
                        label: Some(label),
                    } => {
                        layers.insert(label, id);
                    }
                    _ => {}
                }
            }
            let series = SeriesStore::new(dir.path().join("series"));
            // Laying text out in installed fonts, so no test downloads one.
            let renderer = Renderer::new(RendererConfig {
                font_families: Vec::new(),
            });
            let tools = Tools::new(
                Arc::new(Mutex::new(session)),
                Pipeline::new(PipelineConfig::default()),
                renderer,
            )
            .with_booharu(Arc::new(Checked {
                series: series.clone(),
            }));
            Self {
                _dir: dir,
                tools,
                series,
                page: page.expect("the second page"),
                layers,
            }
        }

        fn id(&self, name: &str) -> LayerId {
            self.layers[name]
        }

        async fn call<T: Tool>(&self, tool: T, args: Value) -> Result<T::Output, T::Error> {
            assert_eq!(crate::tools::refusal(&tool.parameters(), &args), None);
            let args = serde_json::from_value(args).expect("the arguments");
            tool.call(&mut self.tools.tool_context(), args).await
        }

        /// The checks found on the block `name`, by who, each with whether
        /// it's hidden.
        async fn found(&self, name: &str) -> Vec<(String, qc::Source, bool)> {
            let findings = self
                .call(GetQcFindings, json!({"page": self.page}))
                .await
                .expect("the findings");
            findings
                .into_iter()
                .filter(|found| found.finding.layer == self.id(name))
                .map(|found| {
                    assert_eq!(found.page_number, 2);
                    let finding = found.finding;
                    (finding.check, finding.source, finding.hidden)
                })
                .collect()
        }

        /// The undo steps since the chapter was made.
        fn steps(&self) -> Vec<String> {
            let mut steps = self.tools.session.lock().history().steps;
            steps.retain(|step| step != "test");
            steps
        }
    }

    /// The op adding the block `name` to the second page, found and read by
    /// Koharu's stages, reading `source`, and translated into English as
    /// `translation` in a box large enough to read it, when given; locked
    /// when its name says so.
    fn block(name: &str, index: usize, source: &str, translation: Option<&str>) -> Value {
        let y = 10 + 120 * index;
        let mut layer = json!({"kind": "block", "locked": name.starts_with("locked"),
        "source": {
            "frame": {"box": {"bounds": {"x": 10, "y": y, "width": 300, "height": 100}}},
            "content": source,
        },
        "attributes": {"c2pa": {
            "detection": {"action": "c2pa.created"},
            "ocr": {"action": "c2pa.created"},
        }}});
        if let Some(translation) = translation {
            layer["translation"] = json!({"content": translation, "language": "en",
                "frame": {"box": {"bounds": {"x": 10, "y": y, "width": 300, "height": 100},
                    "fit": "best"}},
                "style": {"chars": {"size": 24}}});
        }
        json!({"op": "add_layer", "id": format!("${name}"), "page": "$page", "index": index,
            "layer": layer})
    }

    /// A chapter with an empty line on each of its two pages, which each
    /// page finds.
    fn empty_on_each_page() -> Chapter {
        let mut early = block("early", 0, " ", None);
        early["page"] = json!("$first");
        Chapter::new(vec![early, block("late", 0, " ", None)])
    }

    #[test]
    fn every_quality_check_tool_is_offered_inlined_and_marked_as_it_reads() {
        let definitions = Tools::toolset().tool_definitions();
        for (name, reads) in TOOLS {
            let tool = definitions
                .iter()
                .find(|tool| tool.name.as_str() == name)
                .unwrap_or_else(|| panic!("{name} isn't offered"));
            assert!(!tool.parameters.to_string().contains("$ref"), "{name}");
            let read_only = tool.annotations.read_only_hint == Some(true);
            assert_eq!(read_only, reads, "{name}");
        }
    }

    #[tokio::test]
    async fn a_flag_is_found_as_the_agents_until_it_is_dismissed() {
        let chapter = Chapter::new(vec![block("line", 0, "やあ", Some("Hi"))]);
        assert!(chapter.found("line").await.is_empty());
        let line = chapter.id("line");
        let args = json!({"lines": [{"layer": line, "message": " Check the pun "}]});
        let flagged = chapter.call(FlagLine, args.clone()).await.expect("flagged");
        assert_eq!(
            flagged,
            Flagged {
                flagged: vec![line],
                ..Flagged::default()
            }
        );
        assert_eq!(
            chapter.found("line").await,
            [(qc::FLAG.to_owned(), qc::Source::Agent, false)]
        );
        let findings = chapter
            .call(GetQcFindings, json!({}))
            .await
            .expect("every page's findings");
        let flags: Vec<_> = findings
            .iter()
            .filter(|found| found.finding.layer == line)
            .map(|found| &found.finding.problem)
            .collect();
        assert_eq!(
            flags,
            [&Problem::Flag {
                message: "Check the pun".to_owned()
            }]
        );
        let again = chapter.call(FlagLine, args).await.expect("an answer");
        assert_eq!(again.duplicates, [line]);
        assert_eq!(chapter.steps(), [qc::FLAG_LINE]);

        let other = chapter
            .call(DismissFlag, json!({"layer": line, "message": "Another"}))
            .await
            .expect("an answer");
        assert_eq!(other.dismissed, 0);
        let dismissed = chapter
            .call(
                DismissFlag,
                json!({"layer": line, "message": "Check the pun"}),
            )
            .await
            .expect("dismissed");
        assert_eq!(dismissed.dismissed, 1);
        assert!(chapter.found("line").await.is_empty());
        assert_eq!(chapter.steps(), [qc::FLAG_LINE, qc::DISMISS_FLAG]);
    }

    #[tokio::test]
    async fn a_locked_line_is_left_out_of_the_flags() {
        let chapter = Chapter::new(vec![
            block("line", 0, "やあ", Some("Hi")),
            block("locked", 1, "おい", Some("Hey")),
        ]);
        let (line, locked) = (chapter.id("line"), chapter.id("locked"));
        let args = json!({"lines": [
            {"layer": locked, "message": "Too late"},
            {"layer": line, "message": "Check the pun"},
        ]});
        let flagged = chapter.call(FlagLine, args).await.expect("flagged");
        assert_eq!(
            flagged,
            Flagged {
                flagged: vec![line],
                duplicates: Vec::new(),
                locked: vec![locked],
            }
        );
        assert!(chapter.found("locked").await.is_empty());
        let refused = chapter.call(DismissFlag, json!({"layer": locked})).await;
        assert!(
            refused
                .as_ref()
                .is_err_and(|error| error.to_string().contains(" is locked")),
            "{refused:?}"
        );
    }

    #[tokio::test]
    async fn mark_ok_hides_a_finding_and_undo_ok_brings_it_back() {
        let chapter = Chapter::new(vec![block("empty", 0, " ", None)]);
        let empty = chapter.id("empty");
        let found = |hidden| vec![(qc::EMPTY_SOURCE.to_owned(), qc::Source::Auto, hidden)];
        assert_eq!(chapter.found("empty").await, found(false));
        let args = json!({"layer": empty, "check": qc::EMPTY_SOURCE});
        let marked = chapter.call(MarkQcOk, args.clone()).await.expect("marked");
        assert!(marked.is_some());
        assert_eq!(chapter.found("empty").await, found(true));
        let again = chapter.call(MarkQcOk, args).await.expect("an answer");
        assert_eq!(again, None, "a mark it has");
        let args = json!({"layer": empty, "check": qc::EMPTY_SOURCE, "ok": false});
        let undone = chapter.call(MarkQcOk, args).await.expect("undone");
        assert!(undone.is_some());
        assert_eq!(chapter.found("empty").await, found(false));
        assert_eq!(chapter.steps(), [qc::MARK_QC_OK, qc::CLEAR_QC_OK]);

        let args = json!({"layer": empty, "check": qc::FLAG});
        let refused = chapter.call(MarkQcOk, args).await;
        assert!(
            refused
                .as_ref()
                .is_err_and(|error| error.to_string().contains("booharu_dismiss_flag")),
            "{refused:?}"
        );
    }

    #[tokio::test]
    async fn glossary_slips_are_listed_with_the_term_and_the_lines_translation() {
        let chapter = Chapter::new(vec![
            block("slips", 0, "魔王だ", Some("The king!")),
            block("follows", 1, "魔王め", Some("Demon King!")),
        ]);
        let none = chapter
            .call(CheckGlossary, json!({}))
            .await
            .expect("no slips");
        assert!(none.is_empty(), "no series is linked");
        let mut series = chapter.series.create("Overlord").expect("a series");
        series.glossary = vec![GlossaryEntry::new("魔王", "Demon King")];
        chapter.series.save(&series).expect("saved");
        let file = chapter.tools.session.lock().path().to_owned();
        booharu::context::link_named(&file, &chapter.series, "Overlord", false).expect("linked");

        let slips = chapter
            .call(CheckGlossary, json!({"page": chapter.page}))
            .await
            .expect("the slips");
        let slips = serde_json::to_value(slips).expect("the slips as JSON");
        assert_eq!(
            slips,
            json!([{
                "page_number": 2,
                "page": chapter.page,
                "layer": chapter.id("slips"),
                "check": "glossary:魔王",
                "source": "魔王",
                "target": "Demon King",
                "translation": "The king!",
                "hidden": false,
            }])
        );
        let found = chapter.found("slips").await;
        assert!(
            found.contains(&("glossary:魔王".to_owned(), qc::Source::Auto, false)),
            "{found:?}"
        );
    }

    #[tokio::test]
    async fn the_findings_stop_before_the_next_page_once_stopped() {
        let chapter = empty_on_each_page();
        let pages = |found: Vec<PageFinding>| -> Vec<usize> {
            found.iter().map(|found| found.page_number).collect()
        };
        let every = findings(&chapter.tools.tool_context(), None, || false)
            .await
            .expect("every page's findings");
        assert_eq!(pages(every), [1, 2]);
        let asked = AtomicUsize::new(0);
        let stopped = move || asked.fetch_add(1, Ordering::SeqCst) > 0;
        let first = findings(&chapter.tools.tool_context(), None, stopped)
            .await
            .expect("the first page's findings");
        assert_eq!(pages(first), [1]);
    }

    #[tokio::test]
    async fn the_pages_are_laid_out_holding_no_session() {
        let Chapter { _dir, tools, .. } = empty_on_each_page();
        let session = Arc::downgrade(&tools.session);
        let (reached, mut laying_out) = tokio::sync::mpsc::unbounded_channel();
        let (go, waiting) = mpsc::channel::<()>();
        let context = tools.tool_context();
        let call = tokio::spawn(async move {
            findings(&context, None, move || {
                let _ = reached.send(());
                // No more pages once the test lets go.
                waiting.recv().is_err()
            })
            .await
        });
        laying_out
            .recv()
            .await
            .expect("a page about to be laid out");
        // The call dropped while a page is laid out, as when the agent is
        // stopped, and the project closed.
        call.abort();
        let _ = call.await;
        drop(tools);
        assert!(
            session.upgrade().is_none(),
            "the page loop holds the session"
        );
        drop(go);
    }
}
