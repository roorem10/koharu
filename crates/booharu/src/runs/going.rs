//! Going on: the groups of a plan, one after another, each queued as an
//! ordinary run of Koharu's that follows the run policy, whoever asked for
//! it, the Run button through the desktop command or the agent through its
//! tool, both `booharu_run`, which reach the app through a `RunHost`.
//!
//! One group goes at a time, as one project is open at a time: a call while
//! one goes in the open project answers that one. The next group is queued
//! once the run before it ends Done or DoneWithProblems, on the pages of it
//! the project still has; a run that ends Failed or Stopped ends the group,
//! and so do a Stop that came once the run had ended, the project closing,
//! and another opening, which stops the run going. Each run is followed
//! through `Pipeline::history`, which keeps every event, where the broadcast
//! the interface hears can drop some.
//!
//! A group's follower is spawned with its first run, on the thread that
//! queues it, which no caller can cancel, and frees the group's place as it
//! goes, however it goes: a caller that went away meanwhile, as an agent's
//! tool call does when its chat is stopped, can't leave a group in place
//! that no one follows, which every later call would answer.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::Context as _;
use koharu_pipeline::{Event, Outcome, Pipeline, Request, RunId, Stage, Target};
use koharu_scene::{PageId, ProjectId, Session};
use parking_lot::{Mutex, MutexGuard};
use tokio::{runtime::Handle, task::JoinHandle};

use super::{PlanGroup, RunPlan, RunTarget, all_stages, groups};

/// The app as a group's runs reach it. The desktop app queues a run only on
/// the project open and tells the interface each run; a host outside it,
/// such as the agent's own, queues on its pipeline and tells no one. A host
/// blocks, so it's called on a blocking thread.
pub trait RunHost: Send + Sync + 'static {
    /// The pipeline the runs go on, whose history tells how each ended.
    fn pipeline(&self) -> &Pipeline;

    /// Queues `request` on the project `session` holds while it's the one
    /// open, which it keeps open until the run is queued, so a close after
    /// stops the run; `None` once it closed or another opened.
    fn queue(
        &self,
        session: &Arc<Mutex<Session>>,
        request: Request,
    ) -> anyhow::Result<Option<RunId>>;

    /// Tells whoever follows runs that `run` is the run of the `index`th of
    /// `plan`'s groups.
    fn announce(&self, plan: &RunPlan, index: usize, run: RunId);
}

/// The group going on, if one is. `carry_on` holds it while it answers one
/// or starts one, and a follower while it queues a group's next run or ends
/// the group, so two groups never go at once.
static GOING: Mutex<Option<Going>> = Mutex::new(None);
/// The number of the last group made.
static MADE: AtomicU64 = AtomicU64::new(0);

/// A group going on: its plan, with the run going now, the project it works
/// on, and how many times the pipeline had stopped every run as that run was
/// about to be queued, so a Stop since ends the group, though it came once
/// the run had ended and stopped nothing.
struct Going {
    plan: RunPlan,
    project: ProjectId,
    stops: u64,
}

/// Carries on with what the pages `target` names still need in the project
/// `session` holds when `stages` holds every stage: queues one ordinary run
/// for each group of the pages that need the same stages, one after
/// another, each announced through `host`, and answers the plan with its
/// first run; with nothing left to do, no run and no groups. With fewer
/// stages, one ordinary run of them on every page `target` names, as
/// Koharu's Run button queues it, as a plan of one group. While a group goes
/// on in the project, answers that one, with its run going now, and queues
/// nothing.
pub async fn carry_on(
    host: Arc<dyn RunHost>,
    session: Arc<Mutex<Session>>,
    target: RunTarget,
    stages: Vec<Stage>,
) -> anyhow::Result<RunPlan> {
    let runtime = Handle::current();
    let (plan, _following) =
        tokio::task::spawn_blocking(move || start(host, session, &target, &stages, &runtime))
            .await??;
    Ok(plan)
}

/// `carry_on`'s work, on a thread no caller can cancel: the plan, and the
/// follower of the first run it queued, already spawned on `runtime`.
fn start(
    host: Arc<dyn RunHost>,
    session: Arc<Mutex<Session>>,
    target: &RunTarget,
    stages: &[Stage],
    runtime: &Handle,
) -> anyhow::Result<(RunPlan, Option<JoinHandle<()>>)> {
    let project = session.lock().id();
    let going = GOING.lock();
    if let Some(plan) = answer(going.as_ref(), project) {
        return Ok((plan, None));
    }
    let groups = groups(&session.lock(), target, stages)?;
    let first = groups
        .first()
        .map(|first| request(first_target(target, stages, first), &first.stages));
    go(going, host, session, project, groups, first, runtime)
}

/// Starts a group of `groups` in `project` from `first`, its first group's
/// run, in the place `going` holds: queues the run, announces it, and spawns
/// its follower on `runtime` once the place is let go, which the follower
/// then holds. With no run, a plan of none, which takes no place.
fn go(
    mut going: MutexGuard<'_, Option<Going>>,
    host: Arc<dyn RunHost>,
    session: Arc<Mutex<Session>>,
    project: ProjectId,
    groups: Vec<PlanGroup>,
    first: Option<Request>,
    runtime: &Handle,
) -> anyhow::Result<(RunPlan, Option<JoinHandle<()>>)> {
    let group = MADE.fetch_add(1, Ordering::Relaxed) + 1;
    let Some(first) = first else {
        let plan = RunPlan {
            group,
            groups,
            run: None,
        };
        return Ok((plan, None));
    };
    let stops = host.pipeline().stops();
    let run = host.queue(&session, first)?.context("the project closed")?;
    let plan = RunPlan {
        group,
        groups,
        run: Some(run),
    };
    host.announce(&plan, 0, run);
    *going = Some(Going {
        plan: plan.clone(),
        project,
        stops,
    });
    // Let go first, as a follower that can't be spawned, on a runtime
    // shutting down, frees the place as it's dropped.
    drop(going);
    let following = runtime.spawn(follow(host, session, Place(group), run));
    Ok((plan, Some(following)))
}

/// The group going on in `project`, as `carry_on` answers it.
fn answer(going: Option<&Going>, project: ProjectId) -> Option<RunPlan> {
    going
        .filter(|going| going.project == project)
        .map(|going| going.plan.clone())
}

/// What a group's first run works on: with every stage, the group's pages;
/// with fewer, Koharu's own target for `target`, as Koharu's Run button
/// queues it.
fn first_target(target: &RunTarget, stages: &[Stage], first: &PlanGroup) -> Target {
    match all_stages(stages) {
        true => Target::Pages(first.pages.clone()),
        false => target.target(),
    }
}

/// An ordinary run of `stages` on `target`, with the pipeline's settings.
fn request(target: Target, stages: &[Stage]) -> Request {
    Request {
        target,
        stages: stages.iter().copied().collect(),
        config: None,
    }
}

/// A group's place, which its follower holds: freed as the follower goes,
/// however it goes, unless another group took it meanwhile.
struct Place(u64);

impl Drop for Place {
    fn drop(&mut self) {
        free(self.0);
    }
}

/// Follows the group whose place it holds from `run`, its first: once each
/// run ends, queues the group's next run, or ends the group.
async fn follow(
    host: Arc<dyn RunHost>,
    session: Arc<Mutex<Session>>,
    place: Place,
    mut run: RunId,
) {
    let group = place.0;
    let mut index = 0;
    loop {
        let outcome = match ended(host.pipeline(), run).await {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::warn!("couldn't follow run group {group}: {error:#}");
                return;
            }
        };
        let (working, held) = (Arc::clone(&host), Arc::clone(&session));
        let next =
            tokio::task::spawn_blocking(move || advance(&*working, &held, group, index, &outcome))
                .await;
        match next {
            Ok(Ok(Some(queued))) => (index, run) = queued,
            Ok(Ok(None)) => return,
            Ok(Err(error)) => {
                tracing::warn!("couldn't queue run group {group}'s next run: {error:#}");
                return;
            }
            Err(error) => {
                tracing::warn!("couldn't queue run group {group}'s next run: {error}");
                return;
            }
        }
    }
}

/// How `run` ended, once it has.
async fn ended(pipeline: &Pipeline, run: RunId) -> anyhow::Result<Outcome> {
    loop {
        let told = pipeline.history(run, Duration::from_secs(60)).await?;
        let outcome = told.into_iter().find_map(|event| match event {
            Event::Ended { outcome, .. } => Some(outcome),
            _ => None,
        });
        if let Some(outcome) = outcome {
            return Ok(outcome);
        }
    }
}

/// What follows `group`'s run of its `index`th group ending with `outcome`,
/// in `session`'s project: its next run, queued and announced, with its
/// group's index; `None` once the group is over, which frees its place, or
/// went to another project's group. A Stop since the run was queued ends it
/// as a run that stopped does, and a run that can't be queued ends it too.
fn advance(
    host: &dyn RunHost,
    session: &Arc<Mutex<Session>>,
    group: u64,
    index: usize,
    outcome: &Outcome,
) -> anyhow::Result<Option<(usize, RunId)>> {
    let mut going = GOING.lock();
    let Some(ours) = going.as_mut().filter(|going| going.plan.group == group) else {
        return Ok(None);
    };
    let stops = host.pipeline().stops();
    let outcome = match stops == ours.stops {
        true => outcome,
        false => &Outcome::Stopped,
    };
    let project = session.lock().project();
    let queued = match next_step(&ours.plan.groups, index, outcome, |page| {
        project.page(page).is_ok()
    }) {
        Step::Queue(at, next) => host
            .queue(session, request(Target::Pages(next.pages), &next.stages))
            .map(|run| run.map(|run| (at, run))),
        Step::End => Ok(None),
    };
    match queued {
        Ok(Some((at, run))) => {
            ours.plan.run = Some(run);
            ours.stops = stops;
            host.announce(&ours.plan, at, run);
        }
        _ => *going = None,
    }
    queued
}

/// Frees `group`'s place, unless another group took it.
fn free(group: u64) {
    let mut going = GOING.lock();
    if going
        .as_ref()
        .is_some_and(|going| going.plan.group == group)
    {
        *going = None;
    }
}

/// What a group does once one of its runs ends.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// Queue the `at`th group, on the pages of it the project still has.
    Queue(usize, PlanGroup),
    End,
}

/// What follows the run of the `index`th of `groups` ending with `outcome`:
/// after Done or DoneWithProblems, the next group with a page the project
/// still `has`, on those pages; after a run that failed or stopped, or the
/// last group's, the end.
fn next_step(
    groups: &[PlanGroup],
    index: usize,
    outcome: &Outcome,
    has: impl Fn(PageId) -> bool,
) -> Step {
    if !matches!(outcome, Outcome::Done | Outcome::DoneWithProblems { .. }) {
        return Step::End;
    }
    groups
        .iter()
        .enumerate()
        .skip(index + 1)
        .find_map(|(at, group)| {
            let pages: Vec<PageId> = group
                .pages
                .iter()
                .copied()
                .filter(|&page| has(page))
                .collect();
            (!pages.is_empty()).then(|| {
                Step::Queue(
                    at,
                    PlanGroup {
                        stages: group.stages.clone(),
                        pages,
                    },
                )
            })
        })
        .unwrap_or(Step::End)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use koharu_pipeline::{PageFailure, PipelineConfig};
    use koharu_scene::{Edit, Uuid};
    use serde_json::json;

    use super::*;
    use crate::runs::every_stage;

    fn page(number: u128) -> PageId {
        PageId(Uuid::from_u128(number))
    }

    fn group(stages: &[Stage], pages: &[u128]) -> PlanGroup {
        PlanGroup {
            stages: stages.to_vec(),
            pages: pages.iter().map(|&number| page(number)).collect(),
        }
    }

    const EVERY: [Stage; 4] = [
        Stage::Detect,
        Stage::Recognize,
        Stage::Inpaint,
        Stage::Translate,
    ];

    #[test]
    fn the_next_group_follows_a_run_that_finished() {
        let groups = [
            group(&[Stage::Translate], &[1, 3]),
            group(&EVERY, &[2]),
            group(&[Stage::Inpaint], &[4]),
        ];
        let every = |_| true;
        assert_eq!(
            next_step(&groups, 0, &Outcome::Done, every),
            Step::Queue(1, groups[1].clone())
        );
        let problems = Outcome::DoneWithProblems {
            failures: vec![PageFailure {
                page: page(2),
                stage: Stage::Recognize,
                reason: "the model didn't answer".to_owned(),
            }],
        };
        assert_eq!(
            next_step(&groups, 1, &problems, every),
            Step::Queue(2, groups[2].clone())
        );
        // The last group's run ends the group.
        assert_eq!(next_step(&groups, 2, &Outcome::Done, every), Step::End);
    }

    #[test]
    fn a_run_that_failed_or_stopped_ends_the_group() {
        let groups = [group(&[Stage::Translate], &[1]), group(&EVERY, &[2])];
        let failed = Outcome::Failed {
            page: page(1),
            stage: Stage::Translate,
            reason: "no connection".to_owned(),
        };
        for outcome in [failed, Outcome::Stopped] {
            assert_eq!(next_step(&groups, 0, &outcome, |_| true), Step::End);
        }
    }

    #[test]
    fn a_group_goes_on_with_the_pages_the_project_still_has() {
        let groups = [
            group(&[Stage::Translate], &[1]),
            group(&EVERY, &[2]),
            group(&[Stage::Inpaint], &[3, 4, 5]),
        ];
        // Page 2 and page 4 were removed meanwhile.
        let has = |removed: PageId| ![page(2), page(4)].contains(&removed);
        assert_eq!(
            next_step(&groups, 0, &Outcome::Done, has),
            Step::Queue(2, group(&[Stage::Inpaint], &[3, 5]))
        );
        assert_eq!(next_step(&groups, 0, &Outcome::Done, |_| false), Step::End);
    }

    #[test]
    fn a_call_while_a_group_goes_answers_it_in_its_project_only() {
        let (ours, other) = (ProjectId(Uuid::from_u128(1)), ProjectId(Uuid::from_u128(2)));
        let plan = RunPlan {
            group: 7,
            groups: vec![group(&EVERY, &[1])],
            run: Some(RunId(3)),
        };
        let going = Going {
            plan: plan.clone(),
            project: ours,
            stops: 0,
        };
        assert_eq!(answer(Some(&going), ours), Some(plan));
        assert_eq!(answer(Some(&going), other), None);
        assert_eq!(answer(None, ours), None);
    }

    #[test]
    fn only_every_stage_runs_on_the_planned_pages() {
        let planned = group(&[Stage::Translate], &[2]);
        let chosen = RunTarget::Pages {
            pages: vec![page(2), page(1)],
        };
        assert_eq!(
            first_target(&chosen, &EVERY, &planned),
            Target::Pages(vec![page(2)])
        );
        // Fewer stages run as Koharu's Run button runs them.
        let fewer = [Stage::Translate];
        assert_eq!(
            first_target(&chosen, &fewer, &planned),
            Target::Pages(vec![page(2), page(1)])
        );
        assert_eq!(
            first_target(&RunTarget::Project {}, &fewer, &planned),
            Target::Project
        );
    }

    /// The tests that place groups, one at a time, as the place is the
    /// app's one.
    static PLACING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// A project of `pages` blank pages without a scan, on which a run of
    /// detection ends without loading a model, in a folder removed with it;
    /// and its pages, in order.
    fn chapter(pages: usize) -> (tempfile::TempDir, Arc<Mutex<Session>>, Vec<PageId>) {
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
        let pages = session.project().pages().map(|page| page.id).collect();
        (dir, Arc::new(Mutex::new(session)), pages)
    }

    /// A group of detection on `pages`, which ends without a model.
    fn detection(pages: &[PageId]) -> PlanGroup {
        PlanGroup {
            stages: vec![Stage::Detect],
            pages: pages.to_vec(),
        }
    }

    /// The app as the tests' groups reach it, on a pipeline of its own: the
    /// targets it queued and, for each run it announced, its group and its
    /// group's index, in order.
    struct Host {
        pipeline: Pipeline,
        queued: Mutex<Vec<Target>>,
        announced: Mutex<Vec<(u64, usize)>>,
        /// Whether `queue` finds the project closed, or fails.
        closed: AtomicBool,
        broken: AtomicBool,
        /// Holds `queue` until the test lets it go, once it told it came.
        gate: Option<Gate>,
    }

    struct Gate {
        came: tokio::sync::mpsc::UnboundedSender<()>,
        released: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl Host {
        fn new() -> Self {
            Self {
                pipeline: Pipeline::new(PipelineConfig::default()),
                queued: Mutex::default(),
                announced: Mutex::default(),
                closed: AtomicBool::new(false),
                broken: AtomicBool::new(false),
                gate: None,
            }
        }

        fn queued(&self) -> Vec<Target> {
            self.queued.lock().clone()
        }

        fn announced(&self) -> Vec<(u64, usize)> {
            self.announced.lock().clone()
        }
    }

    impl RunHost for Host {
        fn pipeline(&self) -> &Pipeline {
            &self.pipeline
        }

        fn queue(
            &self,
            session: &Arc<Mutex<Session>>,
            request: Request,
        ) -> anyhow::Result<Option<RunId>> {
            if let Some(gate) = &self.gate {
                gate.came.send(()).expect("the test waits");
                gate.released.lock().recv().expect("the test lets it go");
            }
            if self.broken.load(Ordering::Relaxed) {
                anyhow::bail!("the run settings can't be read");
            }
            if self.closed.load(Ordering::Relaxed) {
                return Ok(None);
            }
            self.queued.lock().push(request.target.clone());
            Ok(Some(self.pipeline.run(Arc::clone(session), request)?))
        }

        fn announce(&self, plan: &RunPlan, index: usize, _: RunId) {
            self.announced.lock().push((plan.group, index));
        }
    }

    /// Waits until no group holds the place, as once its follower went.
    async fn freed() {
        let waited = tokio::time::Instant::now();
        while GOING.lock().is_some() {
            assert!(
                waited.elapsed() < Duration::from_secs(30),
                "the place stays taken"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn a_group_runs_its_groups_one_after_another_then_frees_its_place() {
        let _placing = PLACING.lock().await;
        let (_dir, session, pages) = chapter(2);
        let host = Arc::new(Host::new());
        let project = session.lock().id();
        let groups = vec![detection(&pages[..1]), detection(&pages[1..])];
        let first = request(Target::Pages(vec![pages[0]]), &[Stage::Detect]);
        let held: Arc<dyn RunHost> = host.clone();
        let runtime = Handle::current();
        let (plan, following) = go(
            GOING.lock(),
            held,
            Arc::clone(&session),
            project,
            groups,
            Some(first),
            &runtime,
        )
        .expect("the group");
        // A call while it goes answers it, and queues nothing, whatever it
        // asks for.
        let held: Arc<dyn RunHost> = host.clone();
        let (answered, none) = start(
            held,
            Arc::clone(&session),
            &RunTarget::Project {},
            &every_stage(),
            &runtime,
        )
        .expect("the plan");
        assert!(answered == plan && none.is_none(), "{answered:?}");
        following
            .expect("a follower")
            .await
            .expect("followed to the end");
        assert_eq!(
            host.announced(),
            [(plan.group, 0), (plan.group, 1)],
            "each run with its group's index"
        );
        assert_eq!(
            host.queued(),
            [Target::Pages(vec![pages[0]]), Target::Pages(vec![pages[1]])]
        );
        assert!(GOING.lock().is_none());
    }

    #[tokio::test]
    async fn a_group_ends_at_a_run_failed_or_stopped_a_stop_or_a_run_it_cant_queue() {
        let _placing = PLACING.lock().await;
        let (_dir, session, pages) = chapter(2);
        let host = Host::new();
        let project = session.lock().id();
        // Places a group of two whose first run ended, as of `stops`.
        let place = |stops: u64| {
            let group = MADE.fetch_add(1, Ordering::Relaxed) + 1;
            *GOING.lock() = Some(Going {
                plan: RunPlan {
                    group,
                    groups: vec![detection(&pages[..1]), detection(&pages[1..])],
                    run: Some(RunId(0)),
                },
                project,
                stops,
            });
            group
        };
        let ends = |outcome: &Outcome| {
            let group = place(host.pipeline.stops());
            let next = advance(&host, &session, group, 0, outcome);
            assert!(GOING.lock().is_none(), "{outcome:?}");
            next
        };
        let failed = Outcome::Failed {
            page: pages[0],
            stage: Stage::Detect,
            reason: "no connection".to_owned(),
        };
        for outcome in [failed, Outcome::Stopped] {
            assert_eq!(ends(&outcome).expect("the end"), None);
        }
        host.closed.store(true, Ordering::Relaxed);
        assert_eq!(ends(&Outcome::Done).expect("the end"), None);
        host.closed.store(false, Ordering::Relaxed);
        host.broken.store(true, Ordering::Relaxed);
        assert!(ends(&Outcome::Done).is_err());
        host.broken.store(false, Ordering::Relaxed);
        // A Stop that came once the run had ended, which stopped no run.
        let group = place(host.pipeline.stops());
        host.pipeline.stop();
        let next = advance(&host, &session, group, 0, &Outcome::Done).expect("the end");
        assert!(next.is_none() && GOING.lock().is_none());
        assert!(host.queued().is_empty() && host.announced().is_empty());
        // Another group that took the place meanwhile keeps it.
        let group = place(host.pipeline.stops());
        let theirs = place(host.pipeline.stops());
        let next = advance(&host, &session, group, 0, &Outcome::Done).expect("not ours");
        assert!(next.is_none());
        assert_eq!(
            GOING.lock().take().map(|going| going.plan.group),
            Some(theirs)
        );
    }

    #[tokio::test]
    async fn a_group_whose_caller_went_away_is_followed_to_its_end() {
        let _placing = PLACING.lock().await;
        let (_dir, session, _) = chapter(1);
        let (came, mut coming) = tokio::sync::mpsc::unbounded_channel();
        let (release, released) = std::sync::mpsc::channel();
        let host = Arc::new(Host {
            gate: Some(Gate {
                came,
                released: Mutex::new(released),
            }),
            ..Host::new()
        });
        let calling = tokio::spawn(carry_on(
            host.clone(),
            Arc::clone(&session),
            RunTarget::Project {},
            vec![Stage::Detect],
        ));
        coming.recv().await.expect("the run being queued");
        // The caller goes away while its run is queued, as the agent's tool
        // call does when its chat is stopped.
        calling.abort();
        assert!(calling.await.is_err_and(|error| error.is_cancelled()));
        release.send(()).expect("the run waits to be queued");
        let waited = tokio::time::Instant::now();
        while host.announced().is_empty() {
            assert!(waited.elapsed() < Duration::from_secs(30), "never queued");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        freed().await;
        // So a later call plans again, here finding nothing left to do.
        let plan = carry_on(
            Arc::new(Host::new()),
            session,
            RunTarget::Project {},
            every_stage(),
        )
        .await
        .expect("a plan");
        assert!(plan.run.is_none() && plan.groups.is_empty(), "{plan:?}");
    }
}
