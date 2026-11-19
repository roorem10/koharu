//! Carrying on: `booharu_run`, the Run button's command when all four
//! stages are on, queues the work the pages still need, as
//! `booharu::runs` plans it, one group of pages after another, each as an
//! ordinary run of Koharu's that follows the run policy; with fewer stages,
//! one ordinary run of them, as Koharu's Run button queues it. On a chapter
//! with a linked series, suggested terms, or a brief, a press that would find
//! text and translate translates last, its Translation run its last group.
//!
//! `booharu::runs::carry_on` plans and follows the groups, for this command
//! and the agent's `booharu_run` tool alike, and reaches the app as its
//! `RunHost`: a run is queued only on the project open, and each is told to
//! the interface as `booharu_run_group`.

use std::sync::Arc;

use booharu::runs::{self, RunHost, RunPlan, RunTarget};
use booharu_core::runs::RunStage;
use koharu_pipeline::{Pipeline, Request, RunId};
use koharu_scene::Session;
use parking_lot::Mutex;
use tauri::State;

use crate::{
    app::App,
    booharu::events::{BooharuRunGroup, RUN_GROUP},
    commands::Result,
};

/// Carries on with what the pages `target` names still need when `stages`
/// holds every stage: queues one ordinary run for each group of the pages
/// that need the same stages, one after another, each announced as
/// `booharu_run_group`, and answers the plan with its first run; with
/// nothing left to do, no run and no groups. With fewer stages, one ordinary
/// run of them on every page `target` names, as Koharu's Run button queues
/// it, as a plan of one group. On a chapter with notes, a plan that would
/// find text and translate translates last: every group without Translation
/// first, then one Translation run, leaving out the pages a run gave up on.
/// While a group goes on in the open project, answers that one, with its run
/// going now, and queues nothing. `stages` are the Run menu's: Koharu's four
/// by their names, and Read chapter, which a press leaves out for now.
#[tauri::command]
#[specta::specta]
pub async fn booharu_run(
    app: State<'_, Arc<App>>,
    target: RunTarget,
    stages: Vec<RunStage>,
) -> Result<RunPlan> {
    let app = Arc::clone(&app);
    let session = app.session()?;
    Ok(runs::carry_on(app, session, target, stages).await?)
}

impl RunHost for App {
    fn pipeline(&self) -> &Pipeline {
        &self.pipeline
    }

    fn queue(
        &self,
        session: &Arc<Mutex<Session>>,
        request: Request,
    ) -> anyhow::Result<Option<RunId>> {
        let open = self.project.lock();
        if !open
            .as_ref()
            .is_some_and(|open| Arc::ptr_eq(&open.session, session))
        {
            return Ok(None);
        }
        Ok(Some(self.pipeline.run(Arc::clone(session), request)?))
    }

    fn announce(&self, plan: &RunPlan, index: usize, run: RunId) {
        self.emit(
            RUN_GROUP,
            BooharuRunGroup {
                group: plan.group,
                run,
                index,
                count: plan.groups.len(),
            },
        );
    }
}
