//! Booharu's run policy inside Koharu's scheduler, which a run follows when
//! its snapshot holds one, as every run `Pipeline::run` queues does, whoever
//! queued it; a run without one, as a config only a caller made holds, goes
//! Koharu's way, its first failure ending it.
//!
//! - A page stage that failed is tried again after `retry_wait`, holding no
//!   page in flight and no GPU turn meanwhile, but for what another try can't
//!   help: an invalid page, a model that couldn't load, a hosted reply
//!   koharu-llm already sent again, and a project that couldn't be written,
//!   which ends the run whatever its policy.
//! - A recognition batch's failure is none of its pages' own until a page
//!   fails alone: each of its pages is read again alone, after its wait when
//!   it's tried again, and at once, with no try counted, when it wouldn't be,
//!   so only the page that fails alone is given up on or stops the run.
//! - A page that failed every try is given up on, and the run goes on with
//!   its other pages, ending done with problems; or it ends the run, as
//!   Koharu's first failure does, as the policy says.
//! - A run over pages may go one stage at a time: every page's detection,
//!   then every page's recognition, and so on, starting with no model loaded
//!   and unloading each stage's model before the next stage's loads.
//! - A recognition task reads pages of runs whose filters leave out the same
//!   blocks only ([`alike`]), as it reads them all with its first page's
//!   settings, whatever policy each run has.
//!
//! It also holds `Scheduler::unload_models`, how
//! `Pipeline::booharu_unload_models` frees the models' memory for one loaded
//! outside the pipeline, while no run is queued or going.

use std::collections::{BTreeSet, HashSet};

use booharu_core::runs::{OnPageFailure, RunOrder, RunPolicy, retry_wait};
use koharu_scene::{PageId, ProjectId};
use koharu_torch::Cuda;
use petgraph::{
    Direction::{Incoming, Outgoing},
    stable_graph::{NodeIndex, StableDiGraph},
};
use tokio::{sync::oneshot, time::Instant};

use super::{Candidate, Mode, Run, Scheduler, run::Job};
use crate::{
    Event, Outcome, PageFailure, RunId, Stage,
    booharu::{Class, Failure},
    gpu::{Gpu, Key, Turn},
};

/// A job's tries that failed, when it may go again after the last, and
/// whether it's read alone, as its recognition batch failed.
#[derive(Debug, Default)]
pub(super) struct Tries {
    failed: u32,
    at: Option<Instant>,
    alone: bool,
}

/// The policy `run` follows; none for Koharu's way.
fn policy(run: &Run) -> Option<RunPolicy> {
    run.config.booharu.runs
}

/// Whether `run` goes one stage at a time: a run over pages whose policy
/// says so. A run on blocks or an area, the editor's own on one page, keeps
/// Koharu's order and its models, which reloading for every edit would slow.
fn one_stage_at_a_time(run: &Run) -> bool {
    run.mode == Mode::Batch && policy(run).is_some_and(|policy| policy.order == RunOrder::Stages)
}

/// The stage a run going one stage at a time is on: the earliest it has a
/// job of left, running, waiting to be tried again, or not yet started.
pub(super) fn stage(run: &Run) -> Option<Stage> {
    if !one_stage_at_a_time(run) {
        return None;
    }
    run.graph.node_weights().map(|job| job.stage).min()
}

/// Whether `job`, which its graph has ready, waits: to be tried again after
/// `now`, or, in a run on `stage`, for its own stage.
pub(super) fn held(job: &Job, stage: Option<Stage>, now: Instant) -> bool {
    job.tries.at.is_some_and(|at| at > now) || stage.is_some_and(|stage| job.stage != stage)
}

/// Whether a recognition task may read a page of `other` beside one of
/// `first`, whose settings the task reads every page with: their runs leave
/// out the same blocks, as Redo boxes reads blocks corrected by hand again
/// and another run leaves them as they are.
pub(super) fn alike(first: &Run, other: &Run) -> bool {
    first.config.booharu.redo == other.config.booharu.redo
}

/// When the first job waiting to be tried again is due, after `now`.
pub(super) fn retry(runs: &[Run], now: Instant) -> Option<Instant> {
    runs.iter()
        .flat_map(|run| run.graph.node_weights())
        .filter(|job| !job.running)
        .filter_map(|job| job.tries.at)
        .filter(|&at| at > now)
        .min()
}

/// How `run`, with no job left, ended: done, or done with problems when it
/// gave up on pages.
pub(super) fn outcome(run: &Run) -> Outcome {
    match run.failures.is_empty() {
        true => Outcome::Done,
        false => Outcome::DoneWithProblems {
            failures: run.failures.clone(),
        },
    }
}

/// Each run going one stage at a time, its project, and the stage it's on,
/// as they were before a change, which `Scheduler::upkeep` compares with.
pub(super) fn staged(runs: &[Run]) -> Vec<(RunId, ProjectId, Option<Stage>)> {
    runs.iter()
        .filter(|run| one_stage_at_a_time(run))
        .map(|run| (run.id, run.project, stage(run)))
        .collect()
}

/// Takes `page`'s jobs out of `graph` once the page was given up on, but
/// those running: each task writes its own page's result, which only
/// stopping the run stops, so a stage that ran beside the one that failed,
/// such as its inpainting beside its recognition, still writes what it made
/// and finishes as any other. The page's translation's place in the
/// conversation goes to the next page's, which then follows the page's
/// before it, so the conversation keeps page order across the gap.
fn give_up(graph: &mut StableDiGraph<Job, ()>, page: PageId) {
    let jobs: Vec<NodeIndex> = graph
        .node_indices()
        .filter(|&job| graph[job].page == page && !graph[job].running)
        .collect();
    let translation = jobs
        .iter()
        .copied()
        .find(|&job| graph[job].stage == Stage::Translate);
    if let Some(translation) = translation {
        let neighbour = |direction| {
            graph
                .neighbors_directed(translation, direction)
                .find(|&job| graph[job].stage == Stage::Translate)
        };
        if let (Some(before), Some(after)) = (neighbour(Incoming), neighbour(Outgoing)) {
            graph.add_edge(before, after, ());
        }
    }
    for job in jobs {
        graph.remove_node(job);
    }
}

impl Scheduler {
    /// Whether `candidate` is a page tried again, or one whose recognition
    /// batch failed, which recognition reads alone, so another page's failure
    /// in its batch can't fail it again.
    pub(super) fn alone(&self, candidate: Candidate) -> bool {
        let tries = &self.runs[candidate.run].graph[candidate.job].tries;
        tries.failed > 0 || tries.alone
    }

    /// What `run`, holding some of `jobs`, those of a task at `stage` that
    /// failed with `failure`, does as its policy says: tries each of its
    /// pages again after a wait, while it has tries left and another could
    /// help; or gives up on the pages and goes on with its other pages,
    /// ending done with problems once it has nothing left, which `ended`
    /// then tells. A batch's pages that wouldn't be tried again are read
    /// again alone instead, at once and with no try counted, but after a
    /// model that couldn't load, which fails them all. `run` back for
    /// Koharu's way, ending at once: a run without a policy, a project that
    /// couldn't be written, or a run that stops at a page that failed every
    /// try.
    pub(super) fn failed(
        &mut self,
        mut run: Run,
        jobs: &[(RunId, NodeIndex)],
        stage: Stage,
        failure: &Failure,
        ended: &mut bool,
    ) -> Option<Run> {
        let Some(policy) = policy(&run) else {
            return Some(run);
        };
        if failure.class == Class::Write {
            return Some(run);
        }
        let attempts = u32::from(policy.retries) + 1;
        let again =
            |job: &Job| failure.class == Class::Retryable && job.tries.failed + 1 < attempts;
        // A recognition batch, of this run's pages or others' too, failed
        // with whichever of its pages failed first.
        let batch = jobs.len() > 1 && failure.class != Class::Load;
        let mut ours = Vec::new();
        for &(id, job) in jobs {
            if id != run.id || !run.graph.contains_node(job) {
                continue;
            }
            // A stage that ran on beside one that gave its page up ends with
            // it.
            if run
                .failures
                .iter()
                .any(|given| given.page == run.graph[job].page)
            {
                run.graph.remove_node(job);
            } else {
                ours.push(job);
            }
        }
        if policy.on_page_failure == OnPageFailure::Stop
            && !batch
            && ours.iter().any(|&job| !again(&run.graph[job]))
        {
            return Some(run);
        }
        let now = Instant::now();
        let mut given_up = Vec::new();
        for job in ours {
            let tried_again = again(&run.graph[job]);
            let job = &mut run.graph[job];
            job.running = false;
            if batch && !tried_again {
                job.tries.alone = true;
                continue;
            }
            job.tries.failed += 1;
            if !tried_again {
                given_up.push(job.page);
                continue;
            }
            let attempt = job.tries.failed + 1;
            job.tries.at = Some(now + retry_wait(attempt));
            self.emit(Event::Retrying {
                run: run.id,
                page: job.page,
                stage,
                attempt,
                attempts,
            });
        }
        for page in given_up {
            give_up(&mut run.graph, page);
            run.failures.push(PageFailure {
                page,
                stage,
                reason: failure.reason.clone(),
            });
            self.emit(Event::PageFailed {
                run: run.id,
                page,
                stage,
                reason: failure.reason.clone(),
            });
        }
        if run.graph.node_count() == 0 {
            *ended = true;
            self.emit(Event::Ended {
                run: run.id,
                outcome: outcome(&run),
            });
        } else {
            self.runs.push(run);
        }
        None
    }

    /// Unloads every model when a run that goes one stage at a time comes,
    /// so its first stage's model loads alone, with nothing an earlier run
    /// left: a translator a block's translation loaded, or Generative Fill's
    /// model, which stays five minutes after a run.
    pub(super) fn admitted(&self, run: &Run) {
        if one_stage_at_a_time(run) {
            self.unload(run.project, HashSet::new());
        }
    }

    /// Once a run going one stage at a time moved on from the stage it was
    /// on in `staged`, or ended, unloads every model no queued job needs:
    /// the stage's own, before the next stage's loads, and once the run
    /// ended, every model but another run's.
    pub(super) fn upkeep(&self, staged: Vec<(RunId, ProjectId, Option<Stage>)>) {
        let moved = staged.iter().find(|&&(id, _, was)| {
            self.runs
                .iter()
                .find(|run| run.id == id)
                .is_none_or(|run| stage(run) != was)
        });
        let Some(&(_, project, _)) = moved else {
            return;
        };
        let wanted: HashSet<Key> = self
            .runs
            .iter()
            .flat_map(|run| {
                let left: BTreeSet<Stage> = run.graph.node_weights().map(|job| job.stage).collect();
                left.into_iter()
                    .filter_map(|stage| crate::stages::key(stage, &run.config))
            })
            .collect();
        self.unload(project, wanted);
    }

    /// Unloads every model but `wanted` once the GPU's current call ends,
    /// ahead of every stage's call, for a run of `_project`, which the
    /// scheduler's tests log it by.
    fn unload(&self, _project: ProjectId, wanted: HashSet<Key>) {
        #[cfg(test)]
        crate::booharu::stand_in::unloaded(_project, &wanted);
        self.gpu
            .run(Turn::Upkeep, move |models| models.retain(&wanted));
    }

    /// Frees the models' memory for a model loaded outside the pipeline, as
    /// Read chapter's, unless a run is queued or going, whose models stay:
    /// `unloaded` hears false at once then, and otherwise true once
    /// [`unload_all`] has unloaded them.
    pub(super) fn unload_models(&self, unloaded: oneshot::Sender<bool>) {
        if !self.runs.is_empty() {
            let _ = unloaded.send(false);
            return;
        }
        unload_all(&self.gpu, unloaded);
    }
}

/// Unloads every model once the GPU's current call ends, ahead of every
/// stage's call, as Koharu's `Unload` does, then gives back to the device
/// what libtorch's cache keeps of them, and then tells `unloaded`: llama.cpp,
/// which fits a model loaded outside the pipeline to the memory free, as
/// Read chapter's, counts that cache as used, so that model loads once the
/// answer comes. libtorch is left alone until it has loaded.
fn unload_all(gpu: &Gpu, unloaded: oneshot::Sender<bool>) {
    gpu.run(Turn::Upkeep, move |models| {
        models.retain(&HashSet::new());
        give_back(crate::booharu::libtorch_loaded(), || {
            if Cuda::device_count() > 0 {
                Cuda::empty_cache()?;
            }
            Ok(())
        });
        let _ = unloaded.send(true);
    });
}

/// Empties libtorch's cache by `empty` once libtorch has `started`: before,
/// it isn't loaded, and a call to it would stop the GPU thread.
fn give_back(started: bool, empty: impl FnOnce() -> anyhow::Result<()>) {
    if started && let Err(error) = empty() {
        tracing::warn!("libtorch's cache wasn't emptied: {error}");
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };

    use booharu_core::settings::Snapshot;
    use koharu_scene::{Edit, Session};
    use parking_lot::{Mutex, RwLock};
    use serde_json::json;
    use tokio::sync::mpsc;

    use super::*;
    use crate::{
        PipelineConfig, Target,
        booharu::stand_in::{Entry, Scripted, Try},
        event::Events,
        scheduler::{Command, Plan, Priority},
    };

    const EVERY: [Stage; 4] = [
        Stage::Detect,
        Stage::Recognize,
        Stage::Inpaint,
        Stage::Translate,
    ];

    /// A chapter of blank pages on a scheduler of its own, whose stages are
    /// stand-ins, in a folder removed with it.
    struct Chapter {
        _dir: tempfile::TempDir,
        session: Arc<Mutex<Session>>,
        pages: Vec<PageId>,
        commands: mpsc::UnboundedSender<Command>,
        events: Events,
        script: Scripted,
    }

    impl Chapter {
        fn new(pages: usize) -> Self {
            let dir = tempfile::tempdir().expect("a folder");
            let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
            let ops = (0..pages)
                .map(|index| {
                    serde_json::from_value(json!({
                        "op": "add_page", "index": index, "size": {"width": 100, "height": 100},
                    }))
                    .expect("an op")
                })
                .collect();
            session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops,
                })
                .expect("the pages");
            let project = session.project();
            let events = Events::new();
            let config = Arc::new(RwLock::new(PipelineConfig::default()));
            Self {
                _dir: dir,
                pages: project.pages().map(|page| page.id).collect(),
                script: Scripted::new(project.id()),
                session: Arc::new(Mutex::new(session)),
                commands: Scheduler::spawn(config, events.clone()),
                events,
            }
        }

        /// Queues `stages` on every page as run `id`, following `policy`,
        /// as `Pipeline::run` queues a run.
        fn run(&self, id: u64, stages: &[Stage], policy: Option<RunPolicy>) -> RunId {
            let booharu = Snapshot {
                runs: policy,
                ..Snapshot::default()
            };
            self.run_with(id, stages, booharu)
        }

        /// Queues `stages` on every page as run `id`, with the snapshot
        /// `booharu`.
        fn run_with(&self, id: u64, stages: &[Stage], booharu: Snapshot) -> RunId {
            let run = RunId(id);
            let stages: BTreeSet<Stage> = stages.iter().copied().collect();
            let project = self.session.lock().project();
            let plan = Plan::new(&project, &Target::Project, &stages).expect("a plan");
            self.events.send(Event::Queued {
                run,
                steps: plan.pages() * stages.len(),
                stages: stages.iter().copied().collect(),
                pages: true,
            });
            let config = PipelineConfig {
                booharu,
                ..PipelineConfig::default()
            };
            let queued = Run::new(
                run,
                Arc::clone(&self.session),
                project.id(),
                config,
                plan,
                &stages,
            );
            self.commands
                .send(Command::Run(queued))
                .unwrap_or_else(|_| panic!("the scheduler runs"));
            run
        }

        /// What `run` told, once it ended.
        async fn told(&self, run: RunId) -> Vec<Event> {
            let told = self
                .events
                .history(run, Duration::from_secs(60))
                .await
                .expect("a history");
            assert!(
                matches!(told.last(), Some(Event::Ended { .. })),
                "{told:#?}"
            );
            told
        }

        /// Each stand-in task that ran: its stage, the places of its pages,
        /// and when it started.
        fn ran(&self) -> Vec<(Stage, Vec<usize>, Instant)> {
            self.script
                .log()
                .into_iter()
                .filter_map(|entry| match entry {
                    Entry::Ran { stage, pages, at } => Some((stage, pages, at)),
                    Entry::Unloaded(_) => None,
                })
                .collect()
        }

        /// When the stage of the page at `index` was tried, in order.
        fn tried(&self, index: usize, stage: Stage) -> Vec<Instant> {
            self.ran()
                .into_iter()
                .filter(|(ran, pages, _)| *ran == stage && pages.contains(&index))
                .map(|(.., at)| at)
                .collect()
        }
    }

    fn policy(retries: u8, on_page_failure: OnPageFailure, order: RunOrder) -> Option<RunPolicy> {
        Some(RunPolicy {
            retries,
            on_page_failure,
            order,
        })
    }

    fn dropped() -> anyhow::Error {
        anyhow::anyhow!("the connection dropped")
    }

    fn ended(told: &[Event]) -> Option<&Outcome> {
        match told.last() {
            Some(Event::Ended { outcome, .. }) => Some(outcome),
            _ => None,
        }
    }

    fn retries(told: &[Event]) -> Vec<(PageId, Stage, u32, u32)> {
        told.iter()
            .filter_map(|event| match event {
                Event::Retrying {
                    page,
                    stage,
                    attempt,
                    attempts,
                    ..
                } => Some((*page, *stage, *attempt, *attempts)),
                _ => None,
            })
            .collect()
    }

    fn given_up(told: &[Event]) -> Vec<(PageId, Stage)> {
        told.iter()
            .filter_map(|event| match event {
                Event::PageFailed { page, stage, .. } => Some((*page, *stage)),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn a_page_stage_that_failed_is_tried_again_after_its_wait() {
        let chapter = Chapter::new(2);
        chapter
            .script
            .tries(0, Stage::Detect, vec![Try::Fail(dropped)]);
        let continuing = policy(2, OnPageFailure::Continue, RunOrder::Pages);
        let run = chapter.run(0, &[Stage::Detect, Stage::Recognize], continuing);
        let told = chapter.told(run).await;
        assert_eq!(ended(&told), Some(&Outcome::Done), "{told:#?}");
        assert_eq!(
            retries(&told),
            [(chapter.pages[0], Stage::Detect, 2, 3)],
            "the second of three tries"
        );
        let tried = chapter.tried(0, Stage::Detect);
        assert_eq!(tried.len(), 2);
        assert!(tried[1] - tried[0] >= retry_wait(2), "{tried:?}");
        assert_eq!(chapter.tried(1, Stage::Recognize).len(), 1);
    }

    #[tokio::test]
    async fn what_another_try_cant_help_isnt_tried_again() {
        use koharu_llm::rig::error::{ErrorKind, ErrorReport};
        use rig_agent::run::PromptError;

        fn invalid() -> anyhow::Error {
            crate::Error::invalid("a balloon's outline isn't a path").into()
        }
        fn load() -> anyhow::Error {
            crate::Error::Load {
                model: "lama-manga".to_owned(),
                device: "CUDA".to_owned(),
                free: bytesize::ByteSize(0),
                reserved: bytesize::ByteSize(0),
                active: bytesize::ByteSize(0),
                source: anyhow::anyhow!("out of memory"),
            }
            .into()
        }
        // An overloaded server's reply, which koharu-llm sent again.
        fn sent() -> anyhow::Error {
            PromptError::Report(
                ErrorReport::new(ErrorKind::ProviderResponse, "overloaded").with_http_status(529),
            )
            .into()
        }
        fn unwritten() -> anyhow::Error {
            koharu_scene::Error::Io(std::io::Error::other("the disk is full")).into()
        }

        let chapter = Chapter::new(4);
        let errors: [fn() -> anyhow::Error; 3] = [invalid, load, sent];
        for (index, error) in errors.into_iter().enumerate() {
            chapter
                .script
                .tries(index, Stage::Detect, vec![Try::Fail(error)]);
        }
        let continuing = policy(2, OnPageFailure::Continue, RunOrder::Pages);
        let run = chapter.run(0, &[Stage::Detect, Stage::Recognize], continuing);
        let told = chapter.told(run).await;
        assert!(retries(&told).is_empty(), "{told:#?}");
        let pages = &chapter.pages;
        assert_eq!(
            given_up(&told),
            [
                (pages[0], Stage::Detect),
                (pages[1], Stage::Detect),
                (pages[2], Stage::Detect)
            ]
        );
        let Some(Outcome::DoneWithProblems { failures }) = ended(&told) else {
            panic!("{told:#?}");
        };
        assert_eq!(failures.len(), 3);
        assert_eq!(
            failures[0].reason,
            "detect failed: invalid request: a balloon's outline isn't a path"
        );
        assert_eq!(chapter.tried(3, Stage::Recognize).len(), 1);

        // A project that can't be written ends the run, going on or not.
        chapter
            .script
            .tries(3, Stage::Detect, vec![Try::Fail(unwritten)]);
        let run = chapter.run(1, &[Stage::Detect, Stage::Recognize], continuing);
        let told = chapter.told(run).await;
        assert!(
            retries(&told).is_empty() && given_up(&told).is_empty(),
            "{told:#?}"
        );
        assert!(
            matches!(ended(&told), Some(Outcome::Failed { stage: Stage::Detect, reason, .. })
                if reason == "detect failed: the disk is full"),
            "{told:#?}"
        );
    }

    #[tokio::test]
    async fn a_run_goes_on_past_a_page_that_failed_and_keeps_translating_in_order() {
        let chapter = Chapter::new(4);
        // Without the gap bridged, page 2's translation would be ready while
        // page 0 is still being found.
        let slowly = Try::Slow(Duration::from_millis(300));
        chapter.script.tries(0, Stage::Detect, vec![slowly]);
        chapter
            .script
            .tries(1, Stage::Detect, vec![Try::Fail(dropped)]);
        let continuing = policy(0, OnPageFailure::Continue, RunOrder::Pages);
        let run = chapter.run(0, &[Stage::Detect, Stage::Translate], continuing);
        let told = chapter.told(run).await;
        let failed = chapter.pages[1];
        assert_eq!(given_up(&told), [(failed, Stage::Detect)]);
        assert_eq!(
            ended(&told),
            Some(&Outcome::DoneWithProblems {
                failures: vec![PageFailure {
                    page: failed,
                    stage: Stage::Detect,
                    reason: "detect failed: the connection dropped".to_owned(),
                }],
            })
        );
        let translated: Vec<usize> = chapter
            .ran()
            .into_iter()
            .filter(|(stage, ..)| *stage == Stage::Translate)
            .flat_map(|(_, pages, _)| pages)
            .collect();
        assert_eq!(translated, [0, 2, 3]);
    }

    #[tokio::test]
    async fn a_run_whose_last_page_fails_ends_done_with_problems() {
        let chapter = Chapter::new(1);
        chapter
            .script
            .tries(0, Stage::Detect, vec![Try::Fail(dropped)]);
        let continuing = policy(0, OnPageFailure::Continue, RunOrder::Pages);
        let run = chapter.run(0, &[Stage::Detect, Stage::Recognize], continuing);
        let told = chapter.told(run).await;
        let page = chapter.pages[0];
        assert_eq!(given_up(&told), [(page, Stage::Detect)]);
        assert_eq!(
            ended(&told),
            Some(&Outcome::DoneWithProblems {
                failures: vec![PageFailure {
                    page,
                    stage: Stage::Detect,
                    reason: "detect failed: the connection dropped".to_owned(),
                }],
            })
        );
        assert!(chapter.tried(0, Stage::Recognize).is_empty());
    }

    #[tokio::test]
    async fn a_page_that_fails_its_recognition_batch_fails_alone() {
        // A page holding a locked block, whose edit the project refuses.
        fn locked() -> anyhow::Error {
            koharu_scene::Error::Locked {
                op: 0,
                layer: koharu_scene::LayerId(koharu_scene::Uuid::from_u128(7)),
            }
            .into()
        }
        let reading = [Stage::Recognize, Stage::Translate];
        // A page's failure, the retries it has, and how its reason begins.
        type Case = (fn() -> anyhow::Error, u8, &'static str);
        // Refused, or out of tries, the batch's other pages are read alone
        // at once, with no try counted, and go on.
        let cases: [Case; 2] = [
            (locked, 2, "recognize failed: op 0: layer"),
            (dropped, 0, "recognize failed: the connection dropped"),
        ];
        for (error, allowed, reason) in cases {
            let chapter = Chapter::new(3);
            chapter.script.tries(
                1,
                Stage::Recognize,
                vec![Try::Fail(error), Try::Fail(error)],
            );
            let continuing = policy(allowed, OnPageFailure::Continue, RunOrder::Pages);
            let run = chapter.run(0, &reading, continuing);
            let told = chapter.told(run).await;
            assert!(retries(&told).is_empty(), "{told:#?}");
            assert_eq!(given_up(&told), [(chapter.pages[1], Stage::Recognize)]);
            let Some(Outcome::DoneWithProblems { failures }) = ended(&told) else {
                panic!("{told:#?}");
            };
            assert!(
                failures.len() == 1 && failures[0].reason.starts_with(reason),
                "{failures:?}"
            );
            let read: Vec<Vec<usize>> = chapter
                .ran()
                .into_iter()
                .filter(|(stage, ..)| *stage == Stage::Recognize)
                .map(|(_, pages, _)| pages)
                .collect();
            assert_eq!(read, [vec![0, 1, 2], vec![0], vec![1], vec![2]]);
            assert_eq!(chapter.tried(0, Stage::Translate).len(), 1);
            assert_eq!(chapter.tried(2, Stage::Translate).len(), 1);
        }

        // A run that stops at a failed page stops at the page that failed
        // alone, not at the first of its batch.
        let chapter = Chapter::new(3);
        chapter.script.tries(
            1,
            Stage::Recognize,
            vec![Try::Fail(locked), Try::Fail(locked)],
        );
        let stopping = policy(2, OnPageFailure::Stop, RunOrder::Pages);
        let run = chapter.run(0, &reading, stopping);
        let told = chapter.told(run).await;
        assert!(
            matches!(ended(&told), Some(Outcome::Failed { page, stage: Stage::Recognize, .. })
                if *page == chapter.pages[1]),
            "{told:#?}"
        );
    }

    #[tokio::test]
    async fn a_page_given_up_on_leaves_its_place_in_the_conversation_to_the_next() {
        let chapter = Chapter::new(3);
        let stages: BTreeSet<Stage> = [Stage::Recognize, Stage::Translate].into();
        let project = chapter.session.lock().project();
        let plan = Plan::new(&project, &Target::Project, &stages).expect("a plan");
        let mut run = Run::new(
            RunId(0),
            Arc::clone(&chapter.session),
            project.id(),
            PipelineConfig::default(),
            plan,
            &stages,
        );
        let job = |run: &Run, index: usize, stage: Stage| {
            run.graph
                .node_indices()
                .find(|&job| run.graph[job].index == index && run.graph[job].stage == stage)
        };
        give_up(&mut run.graph, chapter.pages[1]);
        assert_eq!(job(&run, 1, Stage::Recognize), None);
        let first = job(&run, 0, Stage::Translate).expect("page 0's translation");
        let last = job(&run, 2, Stage::Translate).expect("page 2's translation");
        assert!(run.graph.contains_edge(first, last));
    }

    #[tokio::test]
    async fn a_run_that_stops_at_a_failed_page_ends_after_its_tries() {
        let chapter = Chapter::new(2);
        chapter.script.tries(
            0,
            Stage::Detect,
            vec![Try::Fail(dropped), Try::Fail(dropped)],
        );
        let stopping = policy(1, OnPageFailure::Stop, RunOrder::Pages);
        let run = chapter.run(0, &[Stage::Detect], stopping);
        let told = chapter.told(run).await;
        let page = chapter.pages[0];
        assert_eq!(retries(&told), [(page, Stage::Detect, 2, 2)]);
        assert!(given_up(&told).is_empty());
        assert_eq!(
            ended(&told),
            Some(&Outcome::Failed {
                page,
                stage: Stage::Detect,
                reason: "detect failed: the connection dropped".to_owned(),
            })
        );
        assert_eq!(chapter.tried(0, Stage::Detect).len(), 2);
    }

    #[tokio::test]
    async fn stop_ends_a_wait_to_be_tried_again_at_once() {
        let chapter = Chapter::new(1);
        chapter
            .script
            .tries(0, Stage::Detect, vec![Try::Fail(dropped)]);
        let mut events = chapter.events.subscribe();
        let continuing = policy(2, OnPageFailure::Continue, RunOrder::Pages);
        let run = chapter.run(0, &[Stage::Detect], continuing);
        let retrying =
            async { while !matches!(events.recv().await, Ok(Event::Retrying { .. })) {} };
        tokio::time::timeout(Duration::from_secs(30), retrying)
            .await
            .expect("the page waits to be tried again");
        let waited = Instant::now();
        chapter
            .commands
            .send(Command::Stop)
            .unwrap_or_else(|_| panic!("the scheduler runs"));
        let told = chapter.told(run).await;
        assert_eq!(ended(&told), Some(&Outcome::Stopped));
        assert!(waited.elapsed() < retry_wait(2), "{:?}", waited.elapsed());
        assert_eq!(chapter.tried(0, Stage::Detect).len(), 1);
    }

    #[tokio::test]
    async fn one_stage_at_a_time_goes_through_every_page_before_the_next_stage() {
        let chapter = Chapter::new(4);
        let staged = policy(2, OnPageFailure::Continue, RunOrder::Stages);
        let run = chapter.run(0, &EVERY, staged);
        let told = chapter.told(run).await;
        assert_eq!(ended(&told), Some(&Outcome::Done));
        let log = chapter.script.log();
        // Every model leaves first, then each stage's before the next's
        // loads, and the last as the run ends.
        let mut expected = vec![Entry::Unloaded(Vec::new())];
        let mut seen = Vec::new();
        for entry in &log {
            if let Entry::Ran { stage, .. } = entry
                && seen.last() != Some(stage)
            {
                seen.push(*stage);
            }
        }
        assert_eq!(seen, EVERY, "{log:#?}");
        for (at, stage) in EVERY.iter().enumerate() {
            let ran = log
                .iter()
                .filter(|entry| matches!(entry, Entry::Ran { stage: ran, .. } if ran == stage))
                .cloned();
            expected.extend(ran);
            expected.push(Entry::Unloaded(EVERY[at + 1..].to_vec()));
        }
        assert_eq!(log, expected);

        // Koharu's own order overlaps its pages' stages, and unloads nothing.
        let chapter = Chapter::new(4);
        let run = chapter.run(
            0,
            &EVERY,
            policy(2, OnPageFailure::Continue, RunOrder::Pages),
        );
        chapter.told(run).await;
        let log = chapter.script.log();
        assert!(!log.iter().any(|entry| matches!(entry, Entry::Unloaded(_))));
        let stages: Vec<Stage> = chapter.ran().into_iter().map(|(stage, ..)| stage).collect();
        assert!(!stages.is_sorted(), "{stages:?}");
    }

    #[tokio::test]
    async fn without_a_policy_the_first_failure_ends_the_run() {
        let chapter = Chapter::new(2);
        chapter
            .script
            .tries(1, Stage::Detect, vec![Try::Fail(dropped)]);
        let run = chapter.run(0, &[Stage::Detect, Stage::Recognize], None);
        let told = chapter.told(run).await;
        assert!(retries(&told).is_empty() && given_up(&told).is_empty());
        assert_eq!(
            ended(&told),
            Some(&Outcome::Failed {
                page: chapter.pages[1],
                stage: Stage::Detect,
                reason: "detect failed: the connection dropped".to_owned(),
            })
        );
        assert_eq!(chapter.tried(1, Stage::Detect).len(), 1);
        assert!(
            !chapter
                .script
                .log()
                .iter()
                .any(|entry| matches!(entry, Entry::Unloaded(_)))
        );
    }

    #[tokio::test]
    async fn a_recognition_batch_that_failed_reads_its_pages_again_one_by_one() {
        let chapter = Chapter::new(3);
        chapter
            .script
            .tries(1, Stage::Recognize, vec![Try::Fail(dropped)]);
        let continuing = policy(1, OnPageFailure::Continue, RunOrder::Pages);
        let run = chapter.run(0, &[Stage::Recognize], continuing);
        let told = chapter.told(run).await;
        assert_eq!(ended(&told), Some(&Outcome::Done), "{told:#?}");
        assert_eq!(retries(&told).len(), 3);
        let batches: Vec<Vec<usize>> = chapter
            .ran()
            .into_iter()
            .map(|(_, pages, _)| pages)
            .collect();
        assert_eq!(batches, [vec![0, 1, 2], vec![0], vec![1], vec![2]]);
    }

    #[tokio::test]
    async fn a_recognition_task_reads_no_page_of_a_run_whose_filters_differ() {
        let chapter = Chapter::new(3);
        // Cleaning holds every page in flight a while, so both readings'
        // pages are ready to be read together once it's done.
        for index in 0..3 {
            let slow = Try::Slow(Duration::from_millis(300));
            chapter.script.tries(index, Stage::Inpaint, vec![slow]);
        }
        let cleaning = chapter.run(0, &[Stage::Inpaint], None);
        let redo = |redo| Snapshot {
            redo,
            ..Snapshot::default()
        };
        let reading = chapter.run_with(1, &[Stage::Recognize], redo(false));
        // Redo boxes reads blocks corrected by hand, which the other leaves.
        let again = chapter.run_with(2, &[Stage::Recognize], redo(true));
        for run in [cleaning, reading, again] {
            let told = chapter.told(run).await;
            assert_eq!(ended(&told), Some(&Outcome::Done), "{told:#?}");
        }
        let batches: Vec<Vec<usize>> = chapter
            .ran()
            .into_iter()
            .filter(|(stage, ..)| *stage == Stage::Recognize)
            .map(|(_, pages, _)| pages)
            .collect();
        assert_eq!(batches, [vec![0, 1, 2], vec![0, 1, 2]]);
    }

    #[tokio::test]
    async fn unloading_the_models_before_any_loaded_leaves_libtorch_alone() {
        // No model has loaded, so libtorch hasn't: a call to it would panic
        // the GPU thread, which would drop the answer unsent.
        let gpu = Gpu::spawn(tokio::runtime::Handle::current());
        let (unloaded, answer) = oneshot::channel();
        unload_all(&gpu, unloaded);
        assert_eq!(answer.await, Ok(true));
    }

    #[test]
    fn libtorchs_cache_is_emptied_once_libtorch_has_started() {
        give_back(false, || panic!("libtorch isn't loaded yet"));
        let mut emptied = 0;
        give_back(true, || {
            emptied += 1;
            Ok(())
        });
        // A cache that can't be emptied is only logged.
        give_back(true, || anyhow::bail!("the cache is busy"));
        assert_eq!(emptied, 1);
    }

    #[tokio::test]
    async fn the_unload_answers_once_the_gpus_current_call_ends() {
        let gpu = Gpu::spawn(tokio::runtime::Handle::current());
        let (started, running) = oneshot::channel();
        let ended = Arc::new(AtomicBool::new(false));
        let inpainting = Priority {
            mode: Mode::Batch,
            run: RunId(0),
            page: 0,
            stage: Stage::Inpaint,
        };
        gpu.run(Turn::Stage(inpainting), {
            let ended = Arc::clone(&ended);
            move |_| {
                let _ = started.send(());
                std::thread::sleep(Duration::from_millis(300));
                ended.store(true, Ordering::SeqCst);
            }
        });
        running.await.expect("the long call starts");
        let (unloaded, answer) = oneshot::channel();
        unload_all(&gpu, unloaded);
        assert_eq!(answer.await, Ok(true));
        assert!(
            ended.load(Ordering::SeqCst),
            "the answer came while the long call still held its memory"
        );
    }

    #[tokio::test]
    async fn the_models_stay_while_a_run_is_queued_or_going() {
        let chapter = Chapter::new(1);
        let slowly = Try::Slow(Duration::from_millis(300));
        chapter.script.tries(0, Stage::Detect, vec![slowly]);
        let unload = || {
            let (unloaded, answer) = oneshot::channel();
            chapter
                .commands
                .send(Command::BooharuUnload(unloaded))
                .unwrap_or_else(|_| panic!("the scheduler runs"));
            answer
        };
        // A run queued once Read chapter saw none going keeps its models.
        let run = chapter.run(0, &[Stage::Detect], None);
        assert_eq!(unload().await, Ok(false));
        let told = chapter.told(run).await;
        assert_eq!(ended(&told), Some(&Outcome::Done), "{told:#?}");
        // With none queued or going, they go.
        assert_eq!(unload().await, Ok(true));
    }
}
