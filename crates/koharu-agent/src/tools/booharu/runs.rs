//! Carrying on, as the Run button does with every stage on: `booharu_run`
//! plans what the pages still need and queues it, one group of pages after
//! another, through the planner and the groups the desktop command uses,
//! `booharu::runs`. In the app the groups go through the app, which tells
//! the interface each run; outside it, on the tools' own pipeline.

use std::sync::Arc;

use booharu::runs::{self, RunHost, RunPlan, RunTarget};
use koharu_pipeline::{Pipeline, Request, RunId, Stage};
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
    /// The stages; every stage when left out, which carries on with what
    /// the pages still need.
    stages: Option<Vec<Stage>>,
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
         its `stages` and `pages`, and `run`, the first group's run, or none when nothing is \
         left to do. Follow it with wait on `run`. A run that ended failed or stopped ends its \
         group: tell the user so rather than call again. Once a run ended otherwise, call \
         booharu_run again: while its group goes on, it answers the same `group` with the run \
         going now, which you wait on in turn; once the group is over, it plans again from what \
         is still left, as a new `group`, answering no run when nothing is. Stop when it \
         answers no run, or a new `group` in which every page has the same stages it had in the \
         plan before: another run would leave those pages as they are, such as a page where \
         detection finds nothing at all, or one that failed every try, whose `page_failed` said why, \
         so tell the user which pages they are and why instead of calling again. A call while \
         a group goes on answers that group and queues nothing. With `stages` fewer than every \
         stage, it runs those stages on every page `target` names, as Koharu's Run button does, \
         as a plan of one group. Needs a project open, as every tool here does."
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
        let stages = args.stages.unwrap_or_else(runs::every_stage);
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
    use koharu_pipeline::{Event, Outcome, PipelineConfig};
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
            }]
        );
        let run = plan.run.expect("the group's run");
        let told = tools
            .pipeline
            .history(run, Duration::from_secs(30))
            .await
            .expect("the run's events");
        assert_eq!(
            told.last(),
            Some(&Event::Ended {
                run,
                outcome: Outcome::Done,
            })
        );
    }
}
