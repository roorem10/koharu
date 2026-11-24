//! Carrying on: `booharu_run`, the Run button's command when all four
//! stages are on, queues the work the pages still need, as
//! `booharu::runs` plans it, one group of pages after another, each as an
//! ordinary run of Koharu's that follows the run policy; with fewer stages,
//! one ordinary run of them, as Koharu's Run button queues it. On a chapter
//! with a linked series, suggested terms, or a brief, a press that would find
//! text and translate translates last, its Translation run its last group.
//! With Read chapter on, a press that translates on a chapter without a
//! brief reads the chapter just before its Translation run.
//!
//! `booharu::runs::carry_on` plans and follows the groups, for this command
//! and the agent's `booharu_run` tool alike, and reaches the app as its
//! `RunHost`: a run is queued only on the project open, and each is told to
//! the interface as `booharu_run_group`; the read goes through
//! `read_chapter`, as a Read chapter job of the group's, which Stop, closing
//! the project and opening another end, and which writes only while the
//! project it read is the one open; and while Koharu's Gallery is still
//! loading pages into the project, a press that would read is refused.

use std::sync::Arc;

use booharu::{
    context::read::ReadOutcome,
    jobs::{self, JobKind},
    runs::{self, Reading, RunHost, RunPlan, RunTarget},
};
use booharu_core::runs::RunStage;
use koharu_pipeline::{Pipeline, Request, RunId};
use koharu_scene::{PageId, Session};
use parking_lot::Mutex;
use tauri::State;

use crate::{
    app::App,
    booharu::{
        events::{BooharuRunGroup, RUN_GROUP},
        read_chapter::{ReadRequest, pages_loading, read_chapter},
    },
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
/// going now, and queues nothing; while Read chapter reads by hand, refuses,
/// and so does a plan that would read while the Gallery is still loading
/// pages into the chapter. `stages` are the Run menu's: Koharu's four by
/// their names, and `read`, Read chapter: with it, a plan that translates on
/// a chapter without a brief reads the chapter just before its Translation
/// run, every group without Translation first, leaving out the pages a run
/// gave up on, unless by then the chapter has a brief or those pages no
/// text. While it reads, the plan has no run and `reading`, and
/// `booharu_job` tells the read as a job of the group.
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

    fn read(
        self: Arc<Self>,
        session: Arc<Mutex<Session>>,
        group: u64,
        pages: Vec<PageId>,
        stops: u64,
    ) -> Reading {
        Box::pin(async move {
            // Registered first, so Stop and closing the project, which end
            // every job once they stopped the pipeline, reach it from here.
            // Refused, it's told, so the press says why it translates
            // without the read.
            let job = match jobs::start(JobKind::ReadChapter) {
                Ok(job) => job,
                Err(error) => {
                    let told = jobs::refused(JobKind::ReadChapter, Some(group), &error);
                    crate::booharu::jobs::tell(&self, told);
                    return Err(error);
                }
            };
            let open = self
                .project
                .lock()
                .as_ref()
                .is_some_and(|open| Arc::ptr_eq(&open.session, &session));
            if !open || self.pipeline.stops() != stops {
                return Ok(ReadOutcome::Stopped);
            }
            let request = ReadRequest {
                model: None,
                pages: Some(pages),
                group: Some(group),
                since: Some(stops),
            };
            read_chapter(self, session, request, job).await
        })
    }

    fn loading(&self) -> bool {
        pages_loading(self)
    }
}
