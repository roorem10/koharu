//! Carrying on, as the Run button does with every stage on: `booharu_run`
//! plans what the pages still need and queues it, one group of pages after
//! another, through the planner and the groups the desktop command uses,
//! `booharu::runs`. In the app the groups go through the app, which tells
//! the interface each run and reads the chapter for a press that names Read
//! chapter; outside it, on the tools' own pipeline, which never reads. The
//! agent's and MCP's presses read only when they name it.
//!
//! And Redo boxes, as the app's commands do it, through `booharu::redo`:
//! `booharu_count_redo` tells how chosen blocks stand for reading,
//! translating, or cleaning them again, so the agent can ask before it
//! takes the ones fixed by hand too, and `booharu_redo_boxes` queues the one
//! run that does it, as the tools start their runs.

use std::sync::Arc;

use anyhow::Context as _;
use booharu::{
    redo::{self, RedoAnswer, RedoCounts},
    runs::{self, RunHost, RunPlan, RunTarget},
};
use booharu_core::runs::RunStage;
use koharu_pipeline::{Pipeline, Request, RunId, pipeline_stage};
use koharu_scene::{LayerId, Session};
use parking_lot::Mutex;
use rig_core::{
    completion::ToolAnnotations,
    tool::{Tool, ToolContext, ToolExecutionError},
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::{context::blocking, styles::lettering};
use crate::tools::{failed, project, schema};

/// What else Koharu's `wait` tells, as every run follows Booharu's run
/// settings and leaves blocks out by Booharu's rules.
pub(crate) const WAIT: &str = " Under Booharu's run settings a run also tells `retrying`: a \
     page stage failed and waits to be tried again, with the try it waits for and how many it \
     has; and `page_failed`: a page failed every try, with the stage and why, and the run goes \
     on with its other pages. A run that gave up on pages ends `done_with_problems`, listing \
     each. A page stage that left blocks of the page alone tells `skipped` once for the page, \
     with its `stage` and how many it left: `sound_effects` the SFX mode kept as art, \
     `hand_fixed` blocks fixed by hand (for `detect`, the blocks it kept as made or changed by \
     hand), and `locked` ones, `blocks` being whether the run named blocks; tell the user what \
     was left alone, and leave hand-fixed blocks to booharu_redo_boxes, which takes them on \
     purpose once the user agrees.";

/// What else Koharu's `run` does, as every run leaves blocks out by
/// Booharu's rules.
pub(crate) const RUN: &str = " Under Booharu, a run leaves alone locked blocks and the blocks \
     fixed by hand for its stage (a source corrected by hand when reading, a translation \
     written or fixed by hand when translating), translates a sound effect only as the SFX \
     mode (`sfx` in `booharu-lettering`) says and never one with a subtitle, keeping the others \
     as art, and, detecting again, keeps what was made or changed by hand, such as a block \
     drawn, corrected, labelled, noted, or subtitled by hand, or one whose clean-up was hidden \
     by hand, and anything locked; \
     booharu_redo_boxes runs stages again on chosen blocks, the hand-fixed ones too on \
     purpose.";

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RunArgs {
    /// The pages: every page, `{"kind": "project"}`, or some,
    /// `{"kind": "pages", "pages": [...]}`.
    target: RunTarget,
    /// The stages: `detect`, `recognize`, `inpaint`, `read` (Read chapter,
    /// only together with `translate`) and `translate`. Left out, Koharu's
    /// four without `read`, which carries on with what the pages still need.
    stages: Option<Vec<RunStage>>,
}

/// The stages a call runs: those it names, or, left out, Koharu's four,
/// which carry on, without Read chapter, so the agent's and MCP's runs never
/// read unless they ask to.
fn stages(named: Option<Vec<RunStage>>) -> Vec<RunStage> {
    named.unwrap_or_else(|| {
        runs::every_stage()
            .into_iter()
            .map(RunStage::from)
            .collect()
    })
}

pub(super) struct Run;

impl Tool for Run {
    const NAME: &'static str = "booharu_run";
    type Args = RunArgs;
    type Output = RunPlan;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Carry on with the chapter where it stopped, as the Run button does with every stage on: \
         find what each page `target` names still needs, from the records Koharu's stages leave \
         on its blocks, and queue one ordinary run for each group of pages that need the same \
         stages, the groups one after another, each run following Booharu's run settings \
         (`booharu-runs`). Answers the plan at once: its `group` number, `groups`, each with \
         its `stages` and `pages`, `run`, the group's run going now, and `reading`, true while \
         the group reads the chapter instead; no groups and no run when nothing is left to do. \
         Follow it with wait on `run`. A run that ended failed or stopped ends its group: tell \
         the user so rather than call again. Once a run ended otherwise, call booharu_run \
         again: while its group goes on, it answers the same `group` with the run going now, \
         which you wait on in turn; once the group is over, it plans again from what is still \
         left, as a new `group`, answering no groups when nothing is. Stop when it answers no \
         groups, or a new `group` in which every page has the same stages it had in the plan \
         before: another run would leave those pages as they are, such as a page where \
         detection finds nothing at all, or one that failed every try, whose `page_failed` said \
         why, so tell the user which pages they are and why instead of calling again. A call \
         while a group goes on answers that group and queues nothing. With `stages` fewer than \
         Koharu's four, it runs those stages on every page `target` names, as Koharu's Run \
         button does, as a plan of one group. On a chapter with a linked series, suggested \
         terms, or a brief, a plan that would detect or read text and translate it translates \
         last, so the run sends only the glossary terms the chapter uses: every group without \
         translation first, then one group of translation alone, on the pages that still need \
         it once the others went (with fewer stages, on every page named), leaving out any page \
         a run of the group gave up on; a page's stages are then those of every group it's in. \
         `stages` left out are Koharu's four without `read`, so a call reads the chapter only \
         when it names `read`. With `read` and `translate`, a plan that translates, on a \
         chapter without a brief (none, or one with a blank summary), translates last whatever \
         it finds, and reads the chapter just before: a group with `read` true, no `stages`, \
         and every page of the project, whose read leaves out any page a run of the group gave \
         up on and writes the chapter's brief, suggested terms, and each line's lettering \
         category and note, as booharu_read_chapter does. While it reads, `run` is null and \
         `reading` true, and wait can't wait on a read: call booharu_run again after a pause, \
         and it answers the group as it is then, still reading, or with the translation run to \
         wait on. A read that is skipped, as the chapter has a brief by then, the pages have no \
         text, or no model can read them, or that fails, still goes on to translation; a Stop \
         while it reads ends the group. `read` without `translate` changes nothing, and alone \
         runs nothing. booharu_read_chapter reads the chapter by hand, and while it reads, this \
         tool is refused. Its runs leave blocks alone as run's do (locked blocks, blocks fixed \
         by hand, and sound effects the SFX mode keeps as art), as wait's `skipped` tells; \
         booharu_redo_boxes runs stages again on chosen blocks, the hand-fixed ones too on \
         purpose. Needs a project open, as every tool here does."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<RunArgs>()
    }

    async fn call(&self, context: &mut ToolContext, args: RunArgs) -> Result<RunPlan, Self::Error> {
        let tools = project(context)?;
        let host = tools
            .booharu
            .as_ref()
            .and_then(|host| host.runs())
            .unwrap_or_else(|| Arc::new(Alone(tools.pipeline.clone())));
        let stages = stages(args.stages);
        if !stages.iter().any(|&stage| pipeline_stage(stage).is_some()) {
            return Err(failed(
                "A run needs a stage other than `read`: Read chapter reads only just before a \
                 run translates; booharu_read_chapter reads the chapter by hand.",
            ));
        }
        runs::carry_on(host, Arc::clone(&tools.session), args.target, stages)
            .await
            .map_err(|error| failed(format!("{error:#}")))
    }
}

/// A group's runs outside the app: on the tools' own pipeline, on their
/// project, the one open, and told to no one.
struct Alone(Pipeline);

impl RunHost for Alone {
    fn pipeline(&self) -> &Pipeline {
        &self.0
    }

    fn queue(
        &self,
        session: &Arc<Mutex<Session>>,
        request: Request,
    ) -> anyhow::Result<Option<RunId>> {
        Ok(Some(self.0.run(Arc::clone(session), request)?))
    }

    fn announce(&self, _: &RunPlan, _: usize, _: RunId) {}
}

/// A stage Redo boxes runs again.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum RedoStage {
    /// Reading the source again.
    Recognize,
    /// Translating again.
    Translate,
    /// Cleaning the art under the block again.
    Inpaint,
}

impl From<RedoStage> for RunStage {
    fn from(stage: RedoStage) -> Self {
        match stage {
            RedoStage::Recognize => Self::Recognize,
            RedoStage::Translate => Self::Translate,
            RedoStage::Inpaint => Self::Inpaint,
        }
    }
}

fn redo_stages(stages: Vec<RedoStage>) -> Vec<RunStage> {
    stages.into_iter().map(RunStage::from).collect()
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct CountRedoArgs {
    /// The blocks.
    blocks: Vec<LayerId>,
    /// The stages to run again: `recognize`, reading the source,
    /// `translate`, and `inpaint`, cleaning the art under the block.
    #[schemars(length(min = 1))]
    stages: Vec<RedoStage>,
}

pub(super) struct CountRedo;

impl Tool for CountRedo {
    const NAME: &'static str = "booharu_count_redo";
    type Args = CountRedoArgs;
    type Output = RedoCounts;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "How the blocks `blocks` stand now for booharu_redo_boxes running `stages` again, \
         changing nothing: how many the stages would work on as they are (`ready`), how many \
         they would take only with `include_hand_fixed`, as fixed by hand for one of them \
         (`hand_fixed`: a source corrected by hand, for `recognize`; a translation written or \
         fixed by hand, by the user or by you, or a clean-up hidden by hand on a block with no \
         translation, for `translate`), and how many are, or are in, a locked layer, which no \
         redo changes (`locked`). A block named twice counts once. Call it before \
         booharu_redo_boxes, and when it counts any `hand_fixed`, ask the user whether to \
         redo those too. A layer that isn't a block is refused."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<CountRedoArgs>()
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations::default().with_read_only(true)
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: CountRedoArgs,
    ) -> Result<RedoCounts, Self::Error> {
        let CountRedoArgs { blocks, stages } = args;
        let stages = redo_stages(stages);
        blocking(context, move |tools, _| {
            let settings = lettering(tools)?;
            let project = tools.session.lock().project();
            redo::count_redo(&project, &blocks, &stages, &settings)
        })
        .await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RedoBoxesArgs {
    /// The blocks.
    blocks: Vec<LayerId>,
    /// The stages to run again: `recognize`, reading the source,
    /// `translate`, and `inpaint`, cleaning the art under the block.
    #[schemars(length(min = 1))]
    stages: Vec<RedoStage>,
    /// Whether to take the blocks fixed by hand too, replacing the user's
    /// own reading or translation of them; false, the default, leaves them
    /// alone. Set it only once the user agreed.
    #[serde(default)]
    include_hand_fixed: bool,
}

pub(super) struct RedoBoxes;

impl Tool for RedoBoxes {
    const NAME: &'static str = "booharu_redo_boxes";
    type Args = RedoBoxesArgs;
    type Output = RedoAnswer;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Read, translate, or clean chosen blocks again on purpose, as Redo boxes does: one \
         ordinary run of `stages` (`recognize`, reading the source again, `translate`, and \
         `inpaint`, cleaning the art under the block again) on `blocks`, which goes on in the \
         background and which you follow with wait. Every run, those of run and booharu_run \
         included, leaves out a block fixed by hand for its stage: a source corrected by hand \
         when reading, and when translating, a translation written or fixed by hand, by the user \
         or by you, or a clean-up hidden by hand on a block with no translation. This tool takes \
         those too only with `include_hand_fixed`, which replaces the user's own work: call \
         booharu_count_redo first, and when it counts any `hand_fixed`, ask the user before \
         you set it. A locked block, or one in a locked layer, is always left alone, and a \
         sound effect the SFX mode (`sfx` in `booharu-lettering`) keeps as art, or one with a \
         subtitle, stays untranslated, as the run's `skipped` tells. Answers the `run`, none \
         when no block was left to run on, and how many blocks it left alone as `hand_fixed` \
         (none with `include_hand_fixed`) and `locked`; tell the user about any it left alone. \
         A layer that isn't a block is refused."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<RedoBoxesArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: RedoBoxesArgs,
    ) -> Result<RedoAnswer, Self::Error> {
        let RedoBoxesArgs {
            blocks,
            stages,
            include_hand_fixed,
        } = args;
        let stages = redo_stages(stages);
        blocking(context, move |tools, host| {
            let settings = lettering(tools)?;
            let config = host.pipeline_config()?;
            let project = tools.session.lock().project();
            let redo =
                redo::redo_targets(&project, &blocks, &stages, &settings, include_hand_fixed)?;
            // Queued with the session let go, as the run takes it.
            let run = match redo.request(&stages, include_hand_fixed, config) {
                Some(request) => Some(
                    host.start_run(&tools.pipeline, &tools.session, request)?
                        .context("the project closed")?,
                ),
                None => None,
            };
            Ok(redo.answer(run, include_hand_fixed))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, time::Duration};

    use booharu::runs::PlanGroup;
    use booharu_core::settings::BooharuPreferences;
    use koharu_pipeline::{Event, Outcome, PipelineConfig, Stage, Target};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::{Created, Edit, PageId};
    use serde_json::{Map, json};

    use super::*;
    use crate::tools::{Tools, booharu::BooharuHost};

    /// Tools on a project of one blank page, with no scan, so detection
    /// loads no model, outside the app; and the page.
    fn open() -> (tempfile::TempDir, Tools, PageId) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let op = json!({"op": "add_page", "index": 0, "size": {"width": 100, "height": 100}});
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops: vec![serde_json::from_value(op).expect("an op")],
            })
            .expect("a page");
        let Some(&Created::Page { id: page, .. }) = change.created.first() else {
            panic!("{change:?}");
        };
        let tools = Tools::new(
            Arc::new(Mutex::new(session)),
            Pipeline::new(PipelineConfig::default()),
            Renderer::new(RendererConfig::default()),
        );
        (dir, tools, page)
    }

    async fn run(tools: &Tools, args: Value) -> RunPlan {
        let args = serde_json::from_value(args).expect("the arguments");
        Run.call(&mut tools.tool_context(), args)
            .await
            .expect("a plan")
    }

    /// How `run` ended, once it did.
    async fn ended(tools: &Tools, run: RunId) -> Option<Event> {
        let told = tools
            .pipeline
            .history(run, Duration::from_secs(30))
            .await
            .expect("the run's events");
        told.last().cloned()
    }

    #[test]
    fn stages_left_out_are_koharus_four_without_read_chapter() {
        assert_eq!(
            stages(None),
            [
                RunStage::Detect,
                RunStage::Recognize,
                RunStage::Inpaint,
                RunStage::Translate,
            ]
        );
        let named = vec![RunStage::Read, RunStage::Translate];
        assert_eq!(stages(Some(named.clone())), named);
        // The schema names Read chapter among the stages, and takes no
        // other name.
        let schema = Run.parameters();
        let args = |stages: Value| json!({"target": {"kind": "project"}, "stages": stages});
        let every = json!(["detect", "recognize", "inpaint", "read", "translate"]);
        assert_eq!(crate::tools::refusal(&schema, &args(every)), None);
        assert!(crate::tools::refusal(&schema, &args(json!(["reading"]))).is_some());
    }

    #[tokio::test]
    async fn read_chapter_goes_only_with_translation() {
        let (_dir, tools, page) = open();
        // Without translation, Read chapter changes nothing.
        let args =
            json!({"target": {"kind": "pages", "pages": [page]}, "stages": ["detect", "read"]});
        let plan = run(&tools, args).await;
        assert_eq!(
            plan.groups,
            [PlanGroup {
                stages: vec![Stage::Detect],
                pages: vec![page],
                read: false,
            }]
        );
        assert!(!plan.reading, "{plan:?}");
        let run = plan.run.expect("the group's run");
        assert_eq!(
            ended(&tools, run).await,
            Some(Event::Ended {
                run,
                outcome: Outcome::Done,
            })
        );
        // Alone, it runs nothing, and says how to read.
        let args =
            serde_json::from_value(json!({"target": {"kind": "project"}, "stages": ["read"]}))
                .expect("the arguments");
        let error = Run
            .call(&mut tools.tool_context(), args)
            .await
            .expect_err("a run of Read chapter alone");
        assert!(
            error.to_string().contains("booharu_read_chapter"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn a_run_answers_its_plan() {
        let (_dir, tools, page) = open();
        // With every stage it carries on, and a blank page without a scan
        // needs nothing.
        let plan = run(&tools, json!({"target": {"kind": "project"}})).await;
        assert!(plan.groups.is_empty() && plan.run.is_none(), "{plan:?}");
        // Fewer stages run on the pages named, as one group.
        let args = json!({"target": {"kind": "pages", "pages": [page]}, "stages": ["detect"]});
        let plan = run(&tools, args).await;
        assert_eq!(
            plan.groups,
            [PlanGroup {
                stages: vec![Stage::Detect],
                pages: vec![page],
                read: false,
            }]
        );
        let run = plan.run.expect("the group's run");
        assert_eq!(
            ended(&tools, run).await,
            Some(Event::Ended {
                run,
                outcome: Outcome::Done,
            })
        );
    }

    /// The app as Redo boxes reaches it: Booharu's settings at their
    /// defaults, the pipeline's saved settings translating into French, and
    /// the runs it was asked to start, which it never starts, as reading and
    /// translating would load models.
    #[derive(Default)]
    struct App {
        started: Mutex<Vec<Request>>,
    }

    impl BooharuHost for App {
        fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
            Ok(BooharuPreferences::default())
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

        fn pipeline_config(&self) -> anyhow::Result<PipelineConfig> {
            let mut config = PipelineConfig::default();
            config.translator.target = "fr".parse()?;
            Ok(config)
        }

        fn start_run(
            &self,
            _: &Pipeline,
            _: &Arc<Mutex<Session>>,
            request: Request,
        ) -> anyhow::Result<Option<RunId>> {
            self.started.lock().push(request);
            Ok(Some(RunId(7)))
        }
    }

    /// A page holding the blocks `ready`; `read`, whose source was corrected
    /// by hand; `typed`, whose translation was typed by hand; and `locked`,
    /// each found by Koharu's stages; the tools on it in [`App`], the app,
    /// and the blocks' ids.
    fn boxes() -> (tempfile::TempDir, Tools, Arc<App>, impl Fn(&str) -> LayerId) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let block = |name: &str, index: usize, read: bool, locked: bool| {
            let mut c2pa = json!({"detection": {"action": "c2pa.created"}});
            if read {
                c2pa["ocr"] = json!({"action": "c2pa.created"});
            }
            let x = 10.0 + 50.0 * index as f64;
            let mut layer = json!({"kind": "block", "locked": locked, "source": {
                "content": "どうして",
                "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 40.0}}},
            }, "attributes": {"c2pa": c2pa}});
            if name == "typed" {
                layer["translation"] = json!({"content": "Why?"});
            }
            json!({"op": "add_layer", "id": format!("${name}"), "page": "$p", "index": index,
                "layer": layer})
        };
        let ops = [
            json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}),
            block("ready", 0, true, false),
            block("read", 1, false, false),
            block("typed", 2, true, false),
            block("locked", 3, true, true),
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
        let id = move |name: &str| {
            ids.iter()
                .find_map(|(found, id)| (found == name).then_some(*id))
                .expect("a block")
        };
        let app = Arc::new(App::default());
        let tools = Tools::new(
            Arc::new(Mutex::new(session)),
            Pipeline::new(PipelineConfig::default()),
            Renderer::new(RendererConfig::default()),
        )
        .with_booharu(Arc::clone(&app) as Arc<dyn BooharuHost>);
        (dir, tools, app, id)
    }

    #[tokio::test]
    async fn redo_takes_the_hand_fixed_blocks_only_when_asked_and_never_the_locked() {
        let (_dir, tools, app, id) = boxes();
        let names = |names: &[&str]| names.iter().map(|name| id(name)).collect::<Vec<_>>();
        let stages = vec![RedoStage::Recognize, RedoStage::Translate];
        let count = CountRedo
            .call(
                &mut tools.tool_context(),
                CountRedoArgs {
                    blocks: names(&["ready", "read", "typed", "locked", "ready"]),
                    stages: stages.clone(),
                },
            )
            .await
            .expect("a count");
        assert_eq!(
            count,
            RedoCounts {
                ready: 1,
                hand_fixed: 2,
                locked: 1,
            }
        );

        let redo = async |blocks: &[&str], include_hand_fixed| {
            let args = RedoBoxesArgs {
                blocks: names(blocks),
                stages: stages.clone(),
                include_hand_fixed,
            };
            RedoBoxes
                .call(&mut tools.tool_context(), args)
                .await
                .expect("a redo")
        };
        let every = ["ready", "read", "typed", "locked"];
        let answer = |run, hand_fixed| RedoAnswer {
            run,
            hand_fixed,
            locked: 1,
        };
        assert_eq!(redo(&every, false).await, answer(Some(RunId(7)), 2));
        assert_eq!(redo(&every, true).await, answer(Some(RunId(7)), 0));
        // No block left to run on queues nothing.
        assert_eq!(redo(&["locked"], true).await, answer(None, 0));

        // One run of the stages named, with the pipeline's saved settings,
        // whose redo flag lifts the hand-fixed rule only when asked.
        let started = app.started.lock().clone();
        let [without, with] = started.as_slice() else {
            panic!("{started:?}");
        };
        for (request, blocks, redo) in [
            (without, names(&["ready"]), false),
            (with, names(&["ready", "read", "typed"]), true),
        ] {
            assert_eq!(request.target, Target::Blocks(blocks));
            assert_eq!(
                request.stages,
                BTreeSet::from([Stage::Recognize, Stage::Translate])
            );
            let config = request.config.as_ref().expect("the saved settings");
            assert_eq!(config.booharu.redo, redo);
            assert!(!config.booharu.subtitle_run);
            assert_eq!(config.translator.target.to_string(), "fr");
        }
    }

    #[test]
    fn only_reading_translating_and_cleaning_are_redone() {
        let block = "00000000-0000-0000-0000-000000000001";
        for schema in [CountRedo.parameters(), RedoBoxes.parameters()] {
            let args = |stages: Value| json!({"blocks": [block], "stages": stages});
            let every = json!(["recognize", "translate", "inpaint"]);
            assert_eq!(crate::tools::refusal(&schema, &args(every)), None);
            for refused in [json!(["detect"]), json!(["read"]), json!([])] {
                assert!(
                    crate::tools::refusal(&schema, &args(refused.clone())).is_some(),
                    "{refused}"
                );
            }
        }
        // Hand-fixed blocks are left alone unless the call says otherwise.
        let args: RedoBoxesArgs =
            serde_json::from_value(json!({"blocks": [block], "stages": ["translate"]}))
                .expect("the arguments");
        assert!(!args.include_hand_fixed);
        // Counting changes nothing; a redo does.
        assert_eq!(CountRedo.annotations().read_only_hint, Some(true));
        assert_ne!(RedoBoxes.annotations().read_only_hint, Some(true));
    }
}
