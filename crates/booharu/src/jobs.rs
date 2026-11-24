//! Booharu's jobs: Read chapter and Explain this line, which ask a model
//! outside the pipeline, and Fix page levels, which levels pages' scans
//! (`levels`), in a registry for the process, as a run group's place is
//! (`runs`), so Stop and closing the project end them, and a second read, or
//! a second fix, is refused while one goes.
//!
//! A [`Job`] is registered by [`start`] and leaves the registry as it's
//! dropped, however its work ends, cancelling its token as it goes, so work
//! still on a blocking thread writes nothing once its caller went away.
//! [`cancel_all`] cancels every job, which the app calls where Stop and
//! closing the project stop the pipeline; [`cancel`] cancels the jobs of one
//! kind, as the levels dialog's Stop does. The agent's and MCP's stop tool
//! reach a job only through the pipeline's count of Stops, which every model
//! call watches (`calls::Stops`).

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Result, ensure};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::context::read::{ReadSummary, Skip};

/// What a job does.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    ReadChapter,
    Explain,
    /// Fix page levels, which tells no `booharu_job` event: its caller
    /// answers, as Explain's does.
    Levels,
}

/// How a job stands, as the `booharu_job` event tells it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Running,
    Done,
    /// It failed, as its message says, and wrote nothing.
    Failed,
    /// Stop, closing the project, or its caller going away ended it before
    /// it wrote anything.
    Stopped,
    /// It had nothing to do, or nothing to do it with, as its message says.
    Skipped,
}

/// A job as the interface follows it: the `booharu_job` event's payload,
/// told as a read starts, after each of its chunks, and as it ends, always
/// ending with a state other than running, and what `booharu_get_jobs`
/// answers for each read going.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct BooharuJob {
    pub id: u64,
    pub kind: JobKind,
    pub state: JobState,
    /// Why it failed, or was skipped, in English.
    pub message: Option<String>,
    /// Why it was skipped, which the interface words in the user's language.
    pub skip: Option<Skip>,
    /// How many of its steps are done: the chunks of pages read.
    pub step: u32,
    /// How many steps it has; 0 until it knows.
    pub steps: u32,
    /// The run group whose Run step it is, for a read a Run press started.
    pub group: Option<u64>,
    /// What a read did, once it's done.
    pub summary: Option<ReadSummary>,
}

/// Every job going, by id, with its kind and its cancel.
static JOBS: Mutex<BTreeMap<u64, (JobKind, CancellationToken)>> = Mutex::new(BTreeMap::new());
/// The id of the last job made.
static MADE: AtomicU64 = AtomicU64::new(0);

/// A job going: registered until it's dropped, which cancels it.
#[must_use = "a job leaves the registry as it's dropped"]
#[derive(Debug)]
pub struct Job {
    id: u64,
    kind: JobKind,
    cancel: CancellationToken,
}

impl Job {
    /// Its id, unique in the process.
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn kind(&self) -> JobKind {
        self.kind
    }

    /// The token [`cancel_all`] cancels, as dropping the job does.
    pub fn cancel(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Whether it was cancelled.
    pub fn cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.cancel();
        JOBS.lock().remove(&self.id);
    }
}

/// Registers a job of `kind`. A second Read chapter, or a second Fix page
/// levels, while one is going is refused.
pub fn start(kind: JobKind) -> Result<Job> {
    let mut jobs = JOBS.lock();
    let going = jobs.values().any(|(going, _)| *going == kind);
    ensure!(
        kind != JobKind::ReadChapter || !going,
        "Analyze chapter is already running; stop it or wait for it to end."
    );
    ensure!(
        kind != JobKind::Levels || !going,
        "Fix page levels is already running; stop it or wait for it to end."
    );
    let id = MADE.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = CancellationToken::new();
    jobs.insert(id, (kind, cancel.clone()));
    Ok(Job { id, kind, cancel })
}

/// A job of `kind` refused before it could start, as a second read is, as
/// whoever follows jobs is told it: with an id of its own, failed with
/// `why`, for the run group `group` whose Run step it was.
pub fn refused(kind: JobKind, group: Option<u64>, why: &anyhow::Error) -> BooharuJob {
    BooharuJob {
        id: MADE.fetch_add(1, Ordering::Relaxed) + 1,
        kind,
        state: JobState::Failed,
        message: Some(format!("{why:#}")),
        skip: None,
        step: 0,
        steps: 0,
        group,
        summary: None,
    }
}

/// Cancels every job going, as Stop and closing the project do.
pub fn cancel_all() {
    for (_, cancel) in JOBS.lock().values() {
        cancel.cancel();
    }
}

/// Cancels every job of `kind` going, and no other, as the levels dialog's
/// Stop does.
pub fn cancel(kind: JobKind) {
    for (going, cancel) in JOBS.lock().values() {
        if *going == kind {
            cancel.cancel();
        }
    }
}

/// Whether a job of `kind` is going.
pub fn busy(kind: JobKind) -> bool {
    JOBS.lock().values().any(|(going, _)| *going == kind)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Held by every test that registers jobs, places a run group, or fixes
    /// or restores page levels, one at a time, as the registry and the
    /// group's place are the process's: a group is refused while Read
    /// chapter reads, and a fix or a restore of page levels while a group
    /// goes.
    pub(crate) static REGISTRY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[test]
    fn a_second_read_is_refused_and_a_job_leaves_as_its_dropped() -> Result<()> {
        let _held = REGISTRY.blocking_lock();
        let read = start(JobKind::ReadChapter)?;
        assert!(busy(JobKind::ReadChapter));
        let error = start(JobKind::ReadChapter).expect_err("a second read");
        assert!(error.to_string().contains("already running"), "{error}");
        // Explain goes beside it, as often as asked.
        let explain = start(JobKind::Explain)?;
        let again = start(JobKind::Explain)?;
        assert!(busy(JobKind::Explain));
        assert_ne!(explain.id(), again.id());
        let cancel = read.cancel().clone();
        drop(read);
        assert!(cancel.is_cancelled(), "dropped, it's cancelled");
        assert!(!busy(JobKind::ReadChapter));
        let read = start(JobKind::ReadChapter)?;
        drop((explain, again));
        assert!(!busy(JobKind::Explain));
        drop(read);
        Ok(())
    }

    #[test]
    fn a_refused_job_is_told_failed_under_an_id_of_its_own() -> Result<()> {
        let _held = REGISTRY.blocking_lock();
        let read = start(JobKind::ReadChapter)?;
        let why = start(JobKind::ReadChapter).expect_err("a second read");
        let told = refused(JobKind::ReadChapter, Some(3), &why);
        assert_ne!(told.id, read.id(), "never the read going's");
        assert_eq!(
            (told.state, told.group, told.message.as_deref()),
            (JobState::Failed, Some(3), Some(why.to_string().as_str()))
        );
        assert!(busy(JobKind::ReadChapter) && !read.cancelled());
        Ok(())
    }

    #[test]
    fn a_second_fix_of_page_levels_is_refused_and_its_stop_cancels_only_it() -> Result<()> {
        let _held = REGISTRY.blocking_lock();
        let fix = start(JobKind::Levels)?;
        assert!(busy(JobKind::Levels));
        let error = start(JobKind::Levels).expect_err("a second fix");
        assert!(
            error
                .to_string()
                .contains("Fix page levels is already running"),
            "{error}"
        );
        // A read goes beside it, and the levels' Stop leaves the read alone.
        let read = start(JobKind::ReadChapter)?;
        cancel(JobKind::Levels);
        assert!(fix.cancelled() && !read.cancelled());
        drop(fix);
        assert!(!busy(JobKind::Levels));
        let again = start(JobKind::Levels)?;
        assert!(!again.cancelled(), "a new fix starts afresh");
        drop((again, read));
        Ok(())
    }

    #[test]
    fn cancel_all_cancels_every_job_going() -> Result<()> {
        let _held = REGISTRY.blocking_lock();
        let (read, explain) = (start(JobKind::ReadChapter)?, start(JobKind::Explain)?);
        let fix = start(JobKind::Levels)?;
        assert!(!read.cancelled() && !explain.cancelled() && !fix.cancelled());
        cancel_all();
        assert!(read.cancelled() && explain.cancelled() && fix.cancelled());
        // Cancelled, they stay registered until their work lets them go.
        assert!(busy(JobKind::ReadChapter));
        Ok(())
    }
}
