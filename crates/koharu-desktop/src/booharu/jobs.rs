//! Booharu's jobs as the app runs them, Read chapter and Explain this line,
//! which `booharu::jobs` registers: Stop and closing the project end every
//! one ([`stop`], which one marked line in `App::close` and one in
//! `stop_pipeline` call, right after they stop the pipeline; opening and
//! deleting a project close the one open), and each read tells the interface
//! how it goes as `booharu_job` ([`tell`]), which [`booharu_get_jobs`]
//! answers for the reads going, for an interface that loaded late. Explain
//! this line's jobs tell nothing: their command answers.

use std::collections::BTreeMap;

use booharu::jobs::{BooharuJob, JobState};
use parking_lot::Mutex;

use super::events::JOB;
use crate::app::App;

/// The reads going, by id, as last told.
static READS: Mutex<BTreeMap<u64, BooharuJob>> = Mutex::new(BTreeMap::new());

/// Ends every Booharu job going: a read stops before it writes anything,
/// and Explain this line answers that it was stopped.
pub(crate) fn stop() {
    booharu::jobs::cancel_all();
}

/// Tells the interface `job` as `booharu_job`, keeping it for
/// [`booharu_get_jobs`] while it runs.
pub(crate) fn tell(app: &App, job: BooharuJob) {
    keep(&job);
    app.emit(JOB, job);
}

/// Keeps `job` among the reads going while it runs, and lets it go as it
/// ends.
fn keep(job: &BooharuJob) {
    let mut reads = READS.lock();
    match job.state {
        JobState::Running => reads.insert(job.id, job.clone()),
        _ => reads.remove(&job.id),
    };
}

/// The reads going, as they were last told, which the interface starts its
/// jobs from when it loads.
#[tauri::command]
#[specta::specta]
pub fn booharu_get_jobs() -> Vec<BooharuJob> {
    READS.lock().values().cloned().collect()
}

#[cfg(test)]
mod tests {
    use booharu::jobs::JobKind;

    use super::*;

    #[test]
    fn a_read_is_answered_while_it_runs_and_not_once_it_ends() {
        let mut job = BooharuJob {
            id: u64::MAX,
            kind: JobKind::ReadChapter,
            state: JobState::Running,
            message: None,
            skip: None,
            step: 0,
            steps: 2,
            group: None,
            summary: None,
        };
        keep(&job);
        job.step = 1;
        keep(&job);
        let going = |id| {
            booharu_get_jobs()
                .into_iter()
                .filter(|job| job.id == id)
                .collect::<Vec<_>>()
        };
        assert_eq!(going(job.id), [job.clone()], "the last told, once");
        job.state = JobState::Failed;
        keep(&job);
        assert!(going(job.id).is_empty(), "gone as it ends");
    }
}
