//! Carrying on, as the Run button does with every stage on: `booharu_run`
//! plans what the pages still need and queues it, one group of pages after
//! another, through the planner and the groups the desktop command uses,
//! `booharu::runs`. In the app the groups go through the app, which tells
//! the interface each run and reads the chapter for a press that names Read
//! chapter; outside it, on the tools' own pipeline, which never reads. The
//! agent's and MCP's presses read only when they name it.

use std::sync::Arc;

use booharu::runs::{self, RunHost, RunPlan, RunTarget};
use booharu_core::runs::RunStage;
use koharu_pipeline::{Pipeline, Request, RunId, pipeline_stage};
use koharu_scene::Session;
use parking_lot::Mutex;
use rig_core::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::tools::{failed, project, schema};

/// What else Koharu's `wait` tells, as every run follows Booharu's run
/// settings.
pub(crate) const WAIT: &str = " Under Booharu's run settings a run also tells `retrying`: a \
     page stage failed and waits to be tried again, with the try it waits for and how many it \
     has; and `page_failed`: a page failed every try, with the stage and why, and the run goes \
     on with its other pages. A run that gave up on pages ends `done_with_problems`, listing \
     each.";

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
         tool is refused. Needs a project open, as every tool here does."
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use booharu::runs::PlanGroup;
    use koharu_pipeline::{Event, Outcome, PipelineConfig, Stage};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::{Created, Edit, PageId};
    use serde_json::json;

    use super::*;
    use crate::tools::Tools;

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
}
