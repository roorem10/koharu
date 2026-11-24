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
//!
//! A press on a chapter with notes whose plan would find text and translate
//! translates last, as the module above sets out: its plan's last group is
//! the Translation run, queued like the others once the run before it ended,
//! on the pages of it that, carrying on, still need translation then, or,
//! with fewer stages, on each, as Koharu translates every page a run names.
//! A page a run of the press gave up on is left out of every later group,
//! the Translation run's included, and with no page left, the press ends.
//! Each run is announced with its place among the plan's groups, the
//! Translation run's last, so the interface chimes once for the press.
//!
//! A press with Read chapter on reads the chapter before its Translation
//! run, as the module above sets out: the read is a group no run works on,
//! which the follower reads through the host ([`RunHost::read`]) while the
//! group holds its place, its plan `reading` with no `run`, first when it's
//! the plan's first group. Just before it reads, it checks again that the
//! brief is still missing, as the Context tab or the agent may have read the
//! chapter meanwhile, and that its pages have text, skipping the read
//! otherwise. A read done, skipped or failed goes on to the Translation run,
//! as a read never holds translation back, and the app tells why; a read
//! stopped, or a Stop since the group reached it, ends the group, as a run
//! that stopped does. The pages a run of the press gave up on are left out
//! of the read too, and the read is never announced, so the Translation
//! run's announcement stays the last. As the read takes the pages the
//! project has as the press is planned, a press whose plan would read is
//! refused while pages are still loading into the project, as Koharu's
//! Gallery loads a chapter's ([`RunHost::loading`]).

use std::{
    collections::BTreeSet,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::{Context as _, ensure};
use booharu_core::{
    context::{Chapter, ChapterContext, SeriesStore},
    runs::RunStage,
    settings::LetteringSettings,
};
use koharu_pipeline::{Event, Outcome, Pipeline, Request, RunId, Stage, Target, pipeline_stage};
use koharu_scene::{PageId, ProjectId, Session};
use parking_lot::{Mutex, MutexGuard};
use tokio::{runtime::Handle, task::JoinHandle};

use super::{
    PlanGroup, RunPlan, RunTarget, all_stages, brief_missing, groups, has_notes, has_text,
    read_before_translation, to_translate, translate_last,
};
use crate::{
    context::read::{ReadOutcome, Skip},
    jobs::{self, JobKind},
};

/// A Run step's read as a host reads it: how it ended.
pub type Reading = Pin<Box<dyn Future<Output = anyhow::Result<ReadOutcome>> + Send>>;

/// The app as a group's runs reach it. The desktop app queues a run only on
/// the project open and tells the interface each run; a host outside it,
/// such as the agent's own, queues on its pipeline and tells no one. A host
/// blocks, so it's called on a blocking thread; its read, which waits on a
/// model, is awaited.
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

    /// Reads the chapter of the project `session` holds as the Run step of
    /// `group`, on `pages`, those of the project its runs didn't give up on,
    /// while that project is the one open: a Stop since the pipeline's count
    /// of Stops was `stops`, as the group reached the read, closing the
    /// project, or another opening, stops it, writing nothing. Answers how
    /// it ended; skipped or failed, the group goes on to translate, and the
    /// host tells why. A host outside the app skips it, as only the app
    /// reads.
    fn read(
        self: Arc<Self>,
        _session: Arc<Mutex<Session>>,
        _group: u64,
        _pages: Vec<PageId>,
        _stops: u64,
    ) -> Reading {
        Box::pin(async { Ok(ReadOutcome::Skipped(Skip::OnlyInApp)) })
    }

    /// The lettering settings as saved now, `[booharu-lettering]`, read
    /// once for each press, as a run's snapshot reads them: the planner's
    /// sound-effect rule follows the SFX mode. A stored value that doesn't
    /// read reads as its default, and a file that can't be read fails, so
    /// nothing is planned while the mode is unknown.
    fn lettering(&self) -> anyhow::Result<LetteringSettings> {
        LetteringSettings::read()
    }

    /// Whether pages are still loading into the open project, as Koharu's
    /// Gallery loads a chapter's once it opened the chapter as the project:
    /// a press whose plan would read is refused meanwhile, as its read takes
    /// the pages the project has as the press is planned. It's asked while
    /// the press holds the group's place, as `queue` is, so it may take the
    /// open project's lock, never the place. A host outside the app loads
    /// none.
    fn loading(&self) -> bool {
        false
    }
}

/// The group going on, if one is. `carry_on` holds it while it answers one
/// or starts one, and a follower while it queues a group's next run or ends
/// the group, so two groups never go at once. Each takes the session's lock
/// while it holds this, so nothing takes this while it holds the session's
/// lock, as each would wait on the other for good.
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
    /// Whether its last group is the Translation run of a press carrying on
    /// that translates last, which takes only the pages of it that still
    /// need translation once the groups before it went.
    narrows: bool,
    /// The pages its runs gave up on so far, which its later groups leave
    /// out.
    failed: BTreeSet<PageId>,
    /// The lettering settings the press was planned with, which its last
    /// group's narrowing reads too.
    lettering: LetteringSettings,
}

/// The number of the group going on, if one is, in whatever project: its
/// own Run step reads while it goes, and nothing else may. Never asked
/// while the session's lock is held, as a press planning holds the place
/// and takes that lock next.
#[must_use]
pub fn group_going() -> Option<u64> {
    GOING.lock().as_ref().map(|going| going.plan.group)
}

/// Carries on with what the pages `target` names still need in the project
/// `session` holds when `stages` holds Koharu's four: queues one ordinary
/// run for each group of the pages that need the same stages, one after
/// another, each announced through `host`, and answers the plan with its
/// first run; with nothing left to do, no run and no groups. With fewer
/// stages, one ordinary run of them on every page `target` names, as
/// Koharu's Run button queues it, as a plan of one group. On a chapter with
/// notes, a plan that would find text and translate translates last, its
/// Translation run its last group. With Read chapter among `stages`, a plan
/// that translates on a chapter without a brief reads the chapter just
/// before its Translation run, whatever it finds, answering no run and
/// `reading` when it reads first. While a group goes on in the project,
/// answers that one, with its run going now, and queues nothing; while Read
/// chapter reads by hand, refuses, and so does a plan that would read while
/// pages are still loading into the project ([`RunHost::loading`]).
pub async fn carry_on(
    host: Arc<dyn RunHost>,
    session: Arc<Mutex<Session>>,
    target: RunTarget,
    stages: Vec<RunStage>,
) -> anyhow::Result<RunPlan> {
    let runtime = Handle::current();
    let (plan, _following) = tokio::task::spawn_blocking(move || {
        start(host, session, &target, &stages, SeriesStore::open, &runtime)
    })
    .await??;
    Ok(plan)
}

/// `carry_on`'s work, on a thread no caller can cancel: the plan, and the
/// follower of the group it started, already spawned on `runtime`. `store`
/// opens the series folder, when the chapter links a series.
fn start(
    host: Arc<dyn RunHost>,
    session: Arc<Mutex<Session>>,
    target: &RunTarget,
    stages: &[RunStage],
    store: impl FnOnce() -> anyhow::Result<SeriesStore>,
    runtime: &Handle,
) -> anyhow::Result<(RunPlan, Option<JoinHandle<()>>)> {
    let project = session.lock().id();
    let going = GOING.lock();
    if let Some(plan) = answer(going.as_ref(), project) {
        return Ok((plan, None));
    }
    // While Read chapter reads by hand, this press's own read would be
    // refused, and its runs would go on beside the read: one at a time.
    ensure!(
        !jobs::busy(JobKind::ReadChapter),
        "Analyze chapter is running; wait for it to end, or stop it."
    );
    let read = stages.contains(&RunStage::Read);
    let stages: Vec<Stage> = stages.iter().copied().filter_map(pipeline_stage).collect();
    let lettering = host.lettering()?;
    let groups = groups(&session.lock(), target, &stages, &lettering)?;
    let (groups, narrows) = translating(&session, groups, &stages, read, store)?;
    // The read takes the pages the project has now, so while more are still
    // loading into it, a press that would read is refused, as a read by hand
    // is, before it takes the place or queues anything.
    ensure!(
        !(groups.iter().any(|group| group.read) && host.loading()),
        "Analyze chapter can't start while pages are still loading into this chapter; wait until they're in, or run without Analyze chapter."
    );
    let first = groups.first().map(|first| match first.read {
        true => First::Read(first.pages.clone()),
        false => First::Run(Box::new(request(
            first_target(target, &stages, first),
            &first.stages,
        ))),
    });
    go(
        going, host, session, groups, narrows, lettering, first, runtime,
    )
}

/// `groups`, the plan of a press on the project `session` holds with
/// `stages`, as it goes: with `read`, Read chapter on, when a group
/// translates on a chapter whose brief is missing, every group without
/// Translation, then the read, then the Translation run; else translating
/// last, when one would find text and translate on a chapter with notes;
/// as planned otherwise. With whether its Translation run, carrying on,
/// takes only the pages that still need it then: with fewer stages it
/// translates every page of its group, as Koharu translates each page a run
/// names. The chapter is read once, and only for a plan that would split.
/// `store` opens the series folder, when the chapter links a series.
fn translating(
    session: &Mutex<Session>,
    groups: Vec<PlanGroup>,
    stages: &[Stage],
    read: bool,
    store: impl FnOnce() -> anyhow::Result<SeriesStore>,
) -> anyhow::Result<(Vec<PlanGroup>, bool)> {
    let (path, project) = {
        let session = session.lock();
        (session.path().to_owned(), session.project())
    };
    let reads = match read {
        true => read_before_translation(&project, &groups),
        false => None,
    };
    let split = translate_last(&project, &groups);
    if reads.is_none() && split.is_none() {
        return Ok((groups, false));
    }
    let chapter = Chapter::read(&path, store)?;
    let narrows = all_stages(stages);
    Ok(match (reads, split) {
        (Some(reads), _) if brief_missing(&chapter.context) => (reads, narrows),
        (_, Some(split)) if has_notes(&chapter) => (split, narrows),
        _ => (groups, false),
    })
}

/// How a group starts: with its first group's run, or with its read, on
/// those pages.
enum First {
    Run(Box<Request>),
    Read(Vec<PageId>),
}

/// What a group's follower waits on: the run of its `at`th group, or the
/// read of its `at`th group, on those pages, with the pipeline's count of
/// Stops as the group reached it, so a Stop since stops it.
#[derive(Debug, PartialEq, Eq)]
enum Now {
    Run(usize, RunId),
    Read(usize, Vec<PageId>, u64),
}

/// Starts a group of `groups` in the project `session` holds from `first`
/// in the place `going` holds: queues its first group's run and announces
/// it, or, when its first group is the read, takes the place reading; and
/// spawns its follower on `runtime` once the place is let go, which the
/// follower then holds. With no first group, a plan of none, which takes no
/// place. `narrows` tells whether its last group takes only the pages that
/// still need translation then, under `lettering`, the settings the press
/// was planned with.
#[allow(clippy::too_many_arguments)]
fn go(
    mut going: MutexGuard<'_, Option<Going>>,
    host: Arc<dyn RunHost>,
    session: Arc<Mutex<Session>>,
    groups: Vec<PlanGroup>,
    narrows: bool,
    lettering: LetteringSettings,
    first: Option<First>,
    runtime: &Handle,
) -> anyhow::Result<(RunPlan, Option<JoinHandle<()>>)> {
    let group = MADE.fetch_add(1, Ordering::Relaxed) + 1;
    let Some(first) = first else {
        let plan = RunPlan {
            group,
            groups,
            run: None,
            reading: false,
        };
        return Ok((plan, None));
    };
    let (project, stops) = (session.lock().id(), host.pipeline().stops());
    let (plan, now) = match first {
        First::Run(request) => {
            let run = host
                .queue(&session, *request)?
                .context("the project closed")?;
            let plan = RunPlan {
                group,
                groups,
                run: Some(run),
                reading: false,
            };
            host.announce(&plan, 0, run);
            (plan, Now::Run(0, run))
        }
        First::Read(pages) => {
            let plan = RunPlan {
                group,
                groups,
                run: None,
                reading: true,
            };
            (plan, Now::Read(0, pages, stops))
        }
    };
    *going = Some(Going {
        plan: plan.clone(),
        project,
        stops,
        narrows,
        failed: BTreeSet::new(),
        lettering,
    });
    // Let go first, as a follower that can't be spawned, on a runtime
    // shutting down, frees the place as it's dropped.
    drop(going);
    let following = runtime.spawn(follow(host, session, Place(group), now));
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

/// Follows the group whose place it holds from `now`, its first run or its
/// read: once each run or the read ends, goes on to the group's next run or
/// read, or ends the group.
async fn follow(host: Arc<dyn RunHost>, session: Arc<Mutex<Session>>, place: Place, mut now: Now) {
    let group = place.0;
    loop {
        let (index, outcome) = match now {
            Now::Run(index, run) => match ended(host.pipeline(), run).await {
                Ok(outcome) => (index, outcome),
                Err(error) => {
                    tracing::warn!("couldn't follow run group {group}: {error:#}");
                    return;
                }
            },
            Now::Read(index, pages, stops) => {
                (index, read_step(&host, &session, group, pages, stops).await)
            }
        };
        let (working, held) = (Arc::clone(&host), Arc::clone(&session));
        let next =
            tokio::task::spawn_blocking(move || advance(&*working, &held, group, index, &outcome))
                .await;
        match next {
            Ok(Ok(Some(next))) => now = next,
            Ok(Ok(None)) => return,
            Ok(Err(error)) => {
                // Told only here: the interface isn't, and the group ends
                // silent; the next call plans the rest and answers this.
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

/// How `group`'s read on `pages` of the project `session` holds, which the
/// group reached as the pipeline's count of Stops was `stops`, ends for the
/// group: done, skipped or failed, as Done, so the Translation run follows
/// it, as a read never holds translation back; stopped, or a Stop since, as
/// Stopped, which ends the group. Just before it reads, it checks again
/// that the read is wanted ([`wanted`]), and skips it, telling no one,
/// otherwise.
async fn read_step(
    host: &Arc<dyn RunHost>,
    session: &Arc<Mutex<Session>>,
    group: u64,
    pages: Vec<PageId>,
    stops: u64,
) -> Outcome {
    if host.pipeline().stops() != stops {
        return Outcome::Stopped;
    }
    let (held, named) = (Arc::clone(session), pages.clone());
    match tokio::task::spawn_blocking(move || wanted(&held, &named)).await {
        Ok(Ok(true)) => {}
        Ok(Ok(false)) => return Outcome::Done,
        Ok(Err(error)) => {
            // The Translation run that follows reads the same files, and
            // fails as it tells why.
            tracing::warn!("run group {group} goes on without Read chapter: {error:#}");
            return Outcome::Done;
        }
        Err(error) => {
            tracing::warn!("run group {group} goes on without Read chapter: {error}");
            return Outcome::Done;
        }
    }
    match Arc::clone(host)
        .read(Arc::clone(session), group, pages, stops)
        .await
    {
        Ok(ReadOutcome::Done(_)) => Outcome::Done,
        Ok(ReadOutcome::Skipped(why)) => {
            tracing::info!("run group {group} skipped Read chapter: {why}");
            Outcome::Done
        }
        Ok(ReadOutcome::Stopped) => Outcome::Stopped,
        Err(error) => {
            tracing::warn!("run group {group} goes on without Read chapter: {error:#}");
            Outcome::Done
        }
    }
}

/// Whether a press's read on `pages` of the project `session` holds still
/// has something to do: the chapter's brief is still missing, as the
/// Context tab or the agent may have read the chapter, or written a brief,
/// since the press planned, and a block on those pages has text to read. A
/// side file that doesn't read fails.
fn wanted(session: &Mutex<Session>, pages: &[PageId]) -> anyhow::Result<bool> {
    let (path, text) = {
        let session = session.lock();
        (
            session.path().to_owned(),
            has_text(&session.project(), pages),
        )
    };
    Ok(text && brief_missing(&ChapterContext::load(&path)?))
}

/// What follows `group`'s run or read of its `index`th group ending with
/// `outcome`, in `session`'s project: its next run, queued and announced, or
/// its read, with its group's index, on the pages of it the project still
/// has and no run of the group gave up on; `None` once the group is over,
/// which frees its place, or went to another project's group. A Stop since
/// the run was queued, or the read reached, ends it as a run that stopped
/// does, and a run that can't be queued ends it too. While the read goes,
/// the plan has no run and is `reading`.
fn advance(
    host: &dyn RunHost,
    session: &Arc<Mutex<Session>>,
    group: u64,
    index: usize,
    outcome: &Outcome,
) -> anyhow::Result<Option<Now>> {
    let mut going = GOING.lock();
    let Some(ours) = going.as_mut().filter(|going| going.plan.group == group) else {
        return Ok(None);
    };
    let stops = host.pipeline().stops();
    let outcome = match stops == ours.stops {
        true => outcome,
        false => &Outcome::Stopped,
    };
    if let Outcome::DoneWithProblems { failures } = outcome {
        ours.failed
            .extend(failures.iter().map(|failure| failure.page));
    }
    let project = session.lock().project();
    let step = next_step(&ours.plan.groups, index, outcome, |page| {
        project.page(page).is_ok() && !ours.failed.contains(&page)
    });
    let next = narrowed(step, ours, session).and_then(|step| match step {
        Step::Queue(at, next) if next.read => Ok(Some(Now::Read(at, next.pages, stops))),
        Step::Queue(at, next) => host
            .queue(session, request(Target::Pages(next.pages), &next.stages))
            .map(|run| run.map(|run| Now::Run(at, run))),
        Step::End => Ok(None),
    });
    match &next {
        Ok(Some(Now::Run(at, run))) => {
            ours.plan.run = Some(*run);
            ours.plan.reading = false;
            ours.stops = stops;
            host.announce(&ours.plan, *at, *run);
        }
        Ok(Some(Now::Read(..))) => {
            ours.plan.run = None;
            ours.plan.reading = true;
            ours.stops = stops;
        }
        _ => *going = None,
    }
    next
}

/// `step`, the next of the press `going`, as it takes it: when it's the
/// Translation run a press carrying on translates last with, only on the
/// pages of it that still need translation now, and with none left the
/// press ends; any other step as it is.
fn narrowed(step: Step, going: &Going, session: &Mutex<Session>) -> anyhow::Result<Step> {
    match step {
        Step::Queue(at, next) if going.narrows && at + 1 == going.plan.groups.len() => {
            let pages = to_translate(&session.lock(), &next.pages, &going.lettering)?;
            Ok(match pages.is_empty() {
                true => Step::End,
                false => Step::Queue(
                    at,
                    PlanGroup {
                        stages: next.stages,
                        pages,
                        read: false,
                    },
                ),
            })
        }
        step => Ok(step),
    }
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

/// What a group does once one of its runs, or its read, ends.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// Go on to the `at`th group, on the pages of it the project still has:
    /// queue its run, or read.
    Queue(usize, PlanGroup),
    End,
}

/// What follows the run or read of the `index`th of `groups` ending with
/// `outcome`: after Done or DoneWithProblems, the next group with a page the
/// project still `has`, on those pages; after a run that failed or stopped,
/// or the last group's, the end.
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
                        read: group.read,
                    },
                )
            })
        })
        .unwrap_or(Step::End)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use booharu_core::context::{ChapterBrief, GlossaryEntry, side_file};
    use koharu_pipeline::{PageFailure, PipelineConfig};
    use koharu_scene::{Edit, Uuid};
    use serde_json::json;

    use super::*;
    use crate::{context::read::ReadSummary, jobs::tests::REGISTRY};

    fn page(number: u128) -> PageId {
        PageId(Uuid::from_u128(number))
    }

    fn group(stages: &[Stage], pages: &[u128]) -> PlanGroup {
        PlanGroup {
            stages: stages.to_vec(),
            pages: pages.iter().map(|&number| page(number)).collect(),
            read: false,
        }
    }

    const EVERY: [Stage; 4] = [
        Stage::Detect,
        Stage::Recognize,
        Stage::Inpaint,
        Stage::Translate,
    ];

    /// The Run menu's stages for `stages`, with Read chapter when `read`.
    fn run_stages(stages: &[Stage], read: bool) -> Vec<RunStage> {
        let mut run: Vec<RunStage> = stages.iter().copied().map(RunStage::from).collect();
        if read {
            run.push(RunStage::Read);
        }
        run
    }

    /// The read of a press, on `pages`.
    fn reading(pages: &[PageId]) -> PlanGroup {
        PlanGroup {
            stages: Vec::new(),
            pages: pages.to_vec(),
            read: true,
        }
    }

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
    fn the_read_follows_as_a_read_on_the_pages_left() {
        let pages = [page(1), page(2), page(3)];
        let groups = [
            group(&[Stage::Detect], &[1, 2, 3]),
            reading(&pages),
            group(&[Stage::Translate], &[1, 2, 3]),
        ];
        // Page 2 was given up on, or removed.
        let has = |kept: PageId| kept != page(2);
        assert_eq!(
            next_step(&groups, 0, &Outcome::Done, has),
            Step::Queue(1, reading(&[page(1), page(3)]))
        );
        assert_eq!(
            next_step(&groups, 1, &Outcome::Done, has),
            Step::Queue(2, group(&[Stage::Translate], &[1, 3]))
        );
        // A read that stopped ends the group.
        assert_eq!(
            next_step(&groups, 1, &Outcome::Stopped, |_| true),
            Step::End
        );
    }

    #[test]
    fn a_call_while_a_group_goes_answers_it_in_its_project_only() {
        let (ours, other) = (ProjectId(Uuid::from_u128(1)), ProjectId(Uuid::from_u128(2)));
        let plan = RunPlan {
            group: 7,
            groups: vec![group(&EVERY, &[1])],
            run: Some(RunId(3)),
            reading: false,
        };
        let going = Going {
            plan: plan.clone(),
            project: ours,
            stops: 0,
            narrows: false,
            failed: BTreeSet::new(),
            lettering: LetteringSettings::default(),
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
            read: false,
        }
    }

    /// A group of `stage` alone on `pages`.
    fn alone(stage: Stage, pages: &[PageId]) -> PlanGroup {
        PlanGroup {
            stages: vec![stage],
            pages: pages.to_vec(),
            read: false,
        }
    }

    /// A group of translation on `pages`, which ends without a model on
    /// pages without text.
    fn translation(pages: &[PageId]) -> PlanGroup {
        alone(Stage::Translate, pages)
    }

    /// The series folder of a chapter that links no series, which it never
    /// opens.
    fn unlinked() -> anyhow::Result<SeriesStore> {
        anyhow::bail!("the chapter links no series")
    }

    /// Gives the chapter `session` holds a suggested term, which its
    /// translation runs send.
    fn noted(session: &Mutex<Session>) {
        let path = session.lock().path().to_owned();
        ChapterContext::update(&path, |context| {
            context
                .suggestions
                .push(GlossaryEntry::new("先生", "Sensei"));
        })
        .expect("the side file");
    }

    /// Gives the chapter `session` holds a brief with `summary`, as the
    /// Context tab's brief or a read writes one.
    fn briefed(session: &Mutex<Session>, summary: &str) {
        let path = session.lock().path().to_owned();
        ChapterContext::update(&path, |context| {
            context.brief = Some(ChapterBrief {
                summary: summary.to_owned(),
                ..ChapterBrief::default()
            });
        })
        .expect("the side file");
    }

    /// Gives `page` of the project `session` holds a block with text to
    /// read and translate.
    fn written(session: &Mutex<Session>, page: PageId) {
        let op = json!({"op": "add_layer", "page": page, "index": 0, "layer": {
            "kind": "block",
            "source": {"frame": {"box": {"bounds": {"x": 10, "y": 10, "width": 40, "height": 40}}},
                "content": "先生!"},
        }});
        session
            .lock()
            .apply(Edit {
                label: "test".to_owned(),
                ops: vec![serde_json::from_value(op).expect("an op")],
            })
            .expect("the block");
    }

    /// The app as the tests' groups reach it, on a pipeline of its own: the
    /// targets it queued and their stages, and, for each run it announced,
    /// its group and its group's index, in order; and each read it was
    /// asked for, with its group, its pages and the count of Stops it began
    /// from.
    struct Host {
        pipeline: Pipeline,
        queued: Mutex<Vec<Target>>,
        asked: Mutex<Vec<Vec<Stage>>>,
        announced: Mutex<Vec<(u64, usize)>>,
        /// Whether `queue` finds the project closed, or fails.
        closed: AtomicBool,
        broken: AtomicBool,
        /// Holds `queue` until the test lets it go, once it told it came.
        gate: Option<Gate>,
        reads: Mutex<Vec<(u64, Vec<PageId>, u64)>>,
        /// How its reads end.
        answer: Mutex<Answer>,
        /// Holds a read until the test lets it go, once it told it came.
        holding: Option<Holding>,
        /// Whether pages are still loading into its project.
        loading: AtomicBool,
    }

    struct Gate {
        came: tokio::sync::mpsc::UnboundedSender<()>,
        released: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    struct Holding {
        came: tokio::sync::mpsc::UnboundedSender<()>,
        released: Arc<tokio::sync::Notify>,
    }

    /// How a read of the tests' host ends.
    #[derive(Clone, Copy, Debug)]
    enum Answer {
        Done,
        Skipped,
        Failed,
        Stopped,
    }

    impl Host {
        fn new() -> Self {
            Self {
                pipeline: Pipeline::new(PipelineConfig::default()),
                queued: Mutex::default(),
                asked: Mutex::default(),
                announced: Mutex::default(),
                closed: AtomicBool::new(false),
                broken: AtomicBool::new(false),
                gate: None,
                reads: Mutex::default(),
                answer: Mutex::new(Answer::Done),
                holding: None,
                loading: AtomicBool::new(false),
            }
        }

        fn reads(&self) -> Vec<(u64, Vec<PageId>, u64)> {
            self.reads.lock().clone()
        }

        fn queued(&self) -> Vec<Target> {
            self.queued.lock().clone()
        }

        fn asked(&self) -> Vec<Vec<Stage>> {
            self.asked.lock().clone()
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
            let stages = request.stages.iter().copied().collect();
            self.asked.lock().push(stages);
            Ok(Some(self.pipeline.run(Arc::clone(session), request)?))
        }

        fn announce(&self, plan: &RunPlan, index: usize, _: RunId) {
            self.announced.lock().push((plan.group, index));
        }

        fn lettering(&self) -> anyhow::Result<LetteringSettings> {
            Ok(LetteringSettings::default())
        }

        fn loading(&self) -> bool {
            self.loading.load(Ordering::Relaxed)
        }

        fn read(
            self: Arc<Self>,
            _session: Arc<Mutex<Session>>,
            group: u64,
            pages: Vec<PageId>,
            stops: u64,
        ) -> Reading {
            Box::pin(async move {
                self.reads.lock().push((group, pages, stops));
                if let Some(holding) = &self.holding {
                    holding.came.send(()).expect("the test waits");
                    holding.released.notified().await;
                }
                let answer = *self.answer.lock();
                match answer {
                    Answer::Done => Ok(ReadOutcome::Done(ReadSummary::default())),
                    Answer::Skipped => Ok(ReadOutcome::Skipped(Skip::NoText)),
                    Answer::Failed => Err(anyhow::anyhow!("the model didn't answer")),
                    Answer::Stopped => Ok(ReadOutcome::Stopped),
                }
            })
        }
    }

    /// A group [`placed`] in the place, freed as it drops, so a test that
    /// fails while it holds the place leaves it free.
    struct Placed(u64);

    impl Drop for Placed {
        fn drop(&mut self) {
            free(self.0);
        }
    }

    /// Places a group of its own, as a Run press does as it plans.
    fn placed() -> Placed {
        let number = MADE.fetch_add(1, Ordering::Relaxed) + 1;
        *GOING.lock() = Some(Going {
            plan: RunPlan {
                group: number,
                groups: vec![group(&EVERY, &[1])],
                run: None,
                reading: true,
            },
            project: ProjectId(Uuid::from_u128(1)),
            stops: 0,
            narrows: false,
            failed: BTreeSet::new(),
            lettering: LetteringSettings::default(),
        });
        Placed(number)
    }

    #[test]
    fn the_group_going_is_the_one_in_the_place() {
        let _registry = REGISTRY.blocking_lock();
        let group = placed();
        assert_eq!(group_going(), Some(group.0));
        drop(group);
        assert_eq!(group_going(), None);
    }

    /// A restore and a fix of page levels ask whether a press goes before
    /// they take the session's lock, never under it: a press holds the place
    /// as it plans and takes that lock next, so one holding the lock while it
    /// waited on the place would wait on the press for good, and the press on
    /// it. The test holds the place as a press planning does, while each
    /// waits on it, and takes the session's lock as the press would, in time;
    /// each goes on once the place is let go, with no group in it.
    #[test]
    fn a_restore_or_a_fix_waits_on_a_press_planning_without_the_sessions_lock() {
        use crate::levels::{self, LevelsReport, LevelsSkip};

        let _registry = REGISTRY.blocking_lock();
        let (_dir, session, pages) = chapter(1);
        let (session, page) = (&*session, pages[0]);
        let reasons = |report: LevelsReport| -> Vec<LevelsSkip> {
            report
                .skipped
                .iter()
                .map(|skipped| skipped.reason)
                .collect()
        };
        let place = GOING.lock();
        let (planned, restored) = std::thread::scope(|scope| {
            let restoring = scope.spawn(|| levels::restore(session, &[page], || false));
            // The time for the restore to reach the place.
            std::thread::sleep(Duration::from_millis(200));
            let planned = session.try_lock_for(Duration::from_secs(5)).is_some();
            drop(place);
            (planned, restoring.join().expect("the restore"))
        });
        assert!(planned, "the restore held the session's lock as it waited");
        let restored = restored.expect("no group went once the place was let go");
        assert_eq!(reasons(restored), [LevelsSkip::NoOriginal]);
        // A fix's second phase, the place taken as its first ends.
        let job = jobs::start(JobKind::Levels).expect("the fix's job");
        let (levelled, first) = std::sync::mpsc::channel();
        let (taken, held) = std::sync::mpsc::channel();
        let (planned, fixed) = std::thread::scope(|scope| {
            let job = &job;
            let fixing = scope.spawn(move || {
                let settings = booharu_core::settings::LevelsSettings::default();
                levels::fix(
                    session,
                    &[page],
                    &settings,
                    || false,
                    job,
                    |_| {
                        levelled.send(()).expect("the test waits");
                        held.recv().expect("the test took the place");
                    },
                )
            });
            first.recv().expect("the first phase ended");
            let place = GOING.lock();
            taken.send(()).expect("the fix goes on");
            std::thread::sleep(Duration::from_millis(200));
            let planned = session.try_lock_for(Duration::from_secs(5)).is_some();
            drop(place);
            (planned, fixing.join().expect("the fix"))
        });
        assert!(planned, "the fix held the session's lock as it waited");
        let fixed = fixed.expect("no group went once the place was let go");
        assert_eq!(reasons(fixed), [LevelsSkip::NoScan]);
    }

    /// A project in a temporary folder holding one page whose scan is
    /// faded, its ink and its paper both grey, which a fix of page levels
    /// levels.
    fn faded() -> (tempfile::TempDir, Mutex<Session>, PageId) {
        use std::io::Cursor;

        use image::{GrayImage, ImageFormat, Luma};
        use koharu_scene::{Hash, Op};

        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let mut scan = Vec::new();
        GrayImage::from_fn(100, 100, |x, _| Luma([60 + x as u8]))
            .write_to(&mut Cursor::new(&mut scan), ImageFormat::Png)
            .expect("a PNG");
        let page = json!({"op": "add_page", "index": 0, "background": Hash::of(&scan)});
        let ops = vec![
            Op::AddFile { bytes: scan },
            serde_json::from_value(page).expect("an op"),
        ];
        session
            .apply(Edit {
                label: "test".to_owned(),
                ops,
            })
            .expect("the page");
        let page = session.project().pages().next().expect("the page").id;
        (dir, Mutex::new(session), page)
    }

    /// A Run press or a group carrying on, holding the group's place,
    /// refuses a fix of page levels as it starts, before it levels a page,
    /// and before its second phase, and a restore, each changing nothing;
    /// once the place is free, each goes on.
    #[test]
    fn a_group_in_the_place_refuses_a_fix_or_a_restore_of_page_levels() {
        use booharu_core::settings::LevelsSettings;

        use crate::levels::{self, RUN_GOING};

        fn refused<T: std::fmt::Debug>(result: anyhow::Result<T>) -> String {
            format!("{:#}", result.expect_err("refused while a group goes"))
        }

        let _registry = REGISTRY.blocking_lock();
        let (_dir, session, page) = faded();
        let settings = LevelsSettings::default();
        // The page's scan, and the history's steps.
        let now = || {
            let session = session.lock();
            let project = session.project();
            let scan = project.page(page).expect("the page").background;
            (scan.expect("a scan").file, session.history().steps)
        };
        let fix = |progress: &mut dyn FnMut()| {
            let job = jobs::start(JobKind::Levels).expect("the fix's job");
            levels::fix(&session, &[page], &settings, || false, &job, |_| progress())
        };
        let imported = now();

        // As a fix starts.
        let group = placed();
        let startable = levels::startable(&[], false);
        let started = fix(&mut || panic!("levelled while a group goes"));
        drop(group);
        assert_eq!(refused(startable), RUN_GOING);
        assert_eq!(refused(started), RUN_GOING);
        assert_eq!(now(), imported);
        // Before its second phase, a press having taken the place as its
        // first ended.
        let mut taken = None;
        let fixed = fix(&mut || taken = Some(placed()));
        drop(taken.expect("the first phase ended"));
        assert_eq!(refused(fixed), RUN_GOING);
        assert_eq!(now(), imported);

        // Once the place is free, a fix goes on.
        levels::startable(&[], false).expect("with no group going");
        assert_eq!(fix(&mut || {}).expect("a fix").changed, [page]);
        let levelled = now();
        assert_ne!(levelled.0, imported.0);
        // And a restore, refused while a group goes.
        let group = placed();
        let restored = levels::restore(&session, &[page], || false);
        drop(group);
        assert_eq!(refused(restored), RUN_GOING);
        assert_eq!(now(), levelled);
        let restored = levels::restore(&session, &[page], || false).expect("a restore");
        assert_eq!(restored.changed, [page]);
        assert_eq!(now().0, imported.0);
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
        let _registry = REGISTRY.lock().await;
        let (_dir, session, pages) = chapter(2);
        let host = Arc::new(Host::new());
        let groups = vec![detection(&pages[..1]), detection(&pages[1..])];
        let first = request(Target::Pages(vec![pages[0]]), &[Stage::Detect]);
        let held: Arc<dyn RunHost> = host.clone();
        let runtime = Handle::current();
        let (plan, following) = go(
            GOING.lock(),
            held,
            Arc::clone(&session),
            groups,
            false,
            LetteringSettings::default(),
            Some(First::Run(Box::new(first))),
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
            &run_stages(&EVERY, true),
            unlinked,
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
        let _registry = REGISTRY.lock().await;
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
                    reading: false,
                },
                project,
                stops,
                narrows: false,
                failed: BTreeSet::new(),
                lettering: LetteringSettings::default(),
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
        let _registry = REGISTRY.lock().await;
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
            vec![RunStage::Detect],
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
            run_stages(&EVERY, true),
        )
        .await
        .expect("a plan");
        assert!(plan.run.is_none() && plan.groups.is_empty(), "{plan:?}");
    }

    #[test]
    fn a_chapter_with_notes_translates_last_and_one_without_runs_as_planned() {
        let (_dir, session, pages) = chapter(2);
        let carrying_on = vec![PlanGroup {
            stages: EVERY.to_vec(),
            pages: pages.clone(),
            read: false,
        }];
        let reading = [Stage::Recognize, Stage::Translate];
        let preset = vec![PlanGroup {
            stages: reading.to_vec(),
            pages: pages.clone(),
            read: false,
        }];
        let plans = |stages: &[Stage], groups: &[PlanGroup]| {
            translating(&session, groups.to_vec(), stages, false, unlinked).expect("the plan")
        };
        assert_eq!(plans(&EVERY, &carrying_on), (carrying_on.clone(), false));
        assert_eq!(plans(&reading, &preset), (preset.clone(), false));
        noted(&session);
        // Carrying on, its Translation run takes only the pages that still
        // need it then; with fewer stages, every page it names.
        let read = PlanGroup {
            stages: EVERY[..3].to_vec(),
            pages: pages.clone(),
            read: false,
        };
        assert_eq!(
            plans(&EVERY, &carrying_on),
            (vec![read, translation(&pages)], true)
        );
        assert_eq!(
            plans(&reading, &preset),
            (
                vec![alone(Stage::Recognize, &pages), translation(&pages)],
                false
            )
        );
        // A plan that finds no text it translates runs as planned.
        let translating_alone = vec![translation(&pages)];
        assert_eq!(
            plans(&[Stage::Translate], &translating_alone),
            (translating_alone, false)
        );
    }

    #[test]
    fn carrying_on_the_translation_run_takes_the_pages_that_still_need_it() {
        let (_dir, session, pages) = chapter(3);
        // A text to translate on the second page alone.
        written(&session, pages[1]);
        let going = |narrows| Going {
            plan: RunPlan {
                group: 0,
                groups: vec![detection(&pages), translation(&pages)],
                run: None,
                reading: false,
            },
            project: session.lock().id(),
            stops: 0,
            narrows,
            failed: BTreeSet::new(),
            lettering: LetteringSettings::default(),
        };
        let last = |pages: &[PageId]| Step::Queue(1, translation(pages));
        assert_eq!(
            narrowed(last(&pages), &going(true), &session).expect("the step"),
            last(&pages[1..2])
        );
        // With fewer stages, every page it names, as Koharu translates them,
        // and a group before the last as it is.
        assert_eq!(
            narrowed(last(&pages), &going(false), &session).expect("the step"),
            last(&pages)
        );
        let first = || Step::Queue(0, detection(&pages));
        assert_eq!(
            narrowed(first(), &going(true), &session).expect("the step"),
            first()
        );
        // With no page left that needs translation, the press ends.
        let none = last(&[pages[0], pages[2]]);
        assert_eq!(
            narrowed(none, &going(true), &session).expect("the step"),
            Step::End
        );
    }

    #[tokio::test]
    async fn a_press_on_a_chapter_with_notes_translates_last_and_chimes_once() {
        let _registry = REGISTRY.lock().await;
        let (_dir, session, pages) = chapter(2);
        noted(&session);
        let host = Arc::new(Host::new());
        let held: Arc<dyn RunHost> = host.clone();
        let reading = [Stage::Recognize, Stage::Translate];
        let (plan, following) = start(
            held,
            Arc::clone(&session),
            &RunTarget::Project {},
            &run_stages(&reading, false),
            unlinked,
            &Handle::current(),
        )
        .expect("the plan");
        assert_eq!(
            plan.groups,
            [alone(Stage::Recognize, &pages), translation(&pages)]
        );
        following
            .expect("a follower")
            .await
            .expect("followed to the end");
        assert_eq!(
            host.asked(),
            [vec![Stage::Recognize], vec![Stage::Translate]]
        );
        assert_eq!(host.queued(), [Target::Project, Target::Pages(pages)]);
        // Each run with its place among the press's groups, the Translation
        // run's last, so the interface chimes once, as that run ends.
        assert_eq!(host.announced(), [(plan.group, 0), (plan.group, 1)]);
        assert_eq!(plan.groups.len(), 2);
        assert!(GOING.lock().is_none());
    }

    #[tokio::test]
    async fn a_press_translating_last_leaves_out_the_pages_a_run_gave_up_on() {
        let _registry = REGISTRY.lock().await;
        let (_dir, session, pages) = chapter(3);
        let host = Host::new();
        let group = MADE.fetch_add(1, Ordering::Relaxed) + 1;
        *GOING.lock() = Some(Going {
            plan: RunPlan {
                group,
                groups: vec![detection(&pages), translation(&pages)],
                run: Some(RunId(0)),
                reading: false,
            },
            project: session.lock().id(),
            stops: host.pipeline.stops(),
            narrows: false,
            failed: BTreeSet::new(),
            lettering: LetteringSettings::default(),
        });
        let problems = Outcome::DoneWithProblems {
            failures: vec![PageFailure {
                page: pages[1],
                stage: Stage::Detect,
                reason: "the model didn't answer".to_owned(),
            }],
        };
        let Some(Now::Run(at, run)) =
            advance(&host, &session, group, 0, &problems).expect("the next run")
        else {
            panic!("no run queued");
        };
        assert_eq!(at, 1);
        assert_eq!(host.queued(), [Target::Pages(vec![pages[0], pages[2]])]);
        assert_eq!(host.asked(), [vec![Stage::Translate]]);
        assert_eq!(host.announced(), [(group, 1)]);
        let outcome = ended(&host.pipeline, run).await.expect("the run's end");
        assert!(matches!(outcome, Outcome::Done), "{outcome:?}");
        let next = advance(&host, &session, group, 1, &outcome).expect("the end");
        assert!(next.is_none() && GOING.lock().is_none());
    }

    #[test]
    fn a_press_with_read_chapter_reads_before_translating_while_the_brief_is_missing() {
        let (_dir, session, pages) = chapter(2);
        let plans = |stages: &[Stage], read: bool, groups: &[PlanGroup]| {
            translating(&session, groups.to_vec(), stages, read, unlinked).expect("the plan")
        };
        let carrying_on = vec![PlanGroup {
            stages: EVERY.to_vec(),
            pages: pages.clone(),
            read: false,
        }];
        let finding = PlanGroup {
            stages: EVERY[..3].to_vec(),
            pages: pages.clone(),
            read: false,
        };
        // Every group without Translation, then the read of every page,
        // then the Translation run, whatever the chapter's notes.
        let read_first = vec![finding.clone(), reading(&pages), translation(&pages)];
        assert_eq!(
            plans(&EVERY, true, &carrying_on),
            (read_first.clone(), true)
        );
        // Off, as before: a chapter without notes runs as planned.
        assert_eq!(
            plans(&EVERY, false, &carrying_on),
            (carrying_on.clone(), false)
        );
        // Stopped while it read, the work left without Translation, then
        // the read, then Translation; with only Translation left, the read
        // first.
        let left = vec![
            PlanGroup {
                stages: vec![Stage::Inpaint, Stage::Translate],
                pages: pages[..1].to_vec(),
                read: false,
            },
            translation(&pages[1..]),
        ];
        assert_eq!(
            plans(&EVERY, true, &left),
            (
                vec![
                    alone(Stage::Inpaint, &pages[..1]),
                    reading(&pages),
                    translation(&pages)
                ],
                true
            )
        );
        let translating_alone = vec![translation(&pages[1..])];
        assert_eq!(
            plans(&EVERY, true, &translating_alone),
            (vec![reading(&pages), translation(&pages[1..])], true)
        );
        // With fewer stages, every page it names, as before.
        let preset = vec![translation(&pages)];
        assert_eq!(
            plans(&[Stage::Translate], true, &preset),
            (vec![reading(&pages), translation(&pages)], false)
        );
        // A press that doesn't translate doesn't read.
        let cleaning = vec![finding.clone()];
        assert_eq!(plans(&EVERY[..3], true, &cleaning), (cleaning, false));
        // A blank summary is no brief.
        briefed(&session, " ");
        assert_eq!(plans(&EVERY, true, &carrying_on), (read_first, true));
        // With a brief, no read: as stage 6 plans it, translating last for
        // the brief.
        briefed(&session, "Hina meets Sensei.");
        assert_eq!(
            plans(&EVERY, true, &carrying_on),
            (vec![finding, translation(&pages)], true)
        );
        assert_eq!(
            plans(&[Stage::Translate], true, &preset),
            (preset.clone(), false)
        );
        // A side file that doesn't read fails the press that would read, as
        // its translation would be refused; one that neither reads nor
        // translates last never reads it.
        let path = session.lock().path().to_owned();
        std::fs::write(side_file(&path), "{").expect("a broken side file");
        let broken = translating(
            &session,
            preset.clone(),
            &[Stage::Translate],
            true,
            unlinked,
        );
        assert!(broken.is_err());
        assert_eq!(plans(&[Stage::Translate], false, &preset), (preset, false));
    }

    #[tokio::test]
    async fn a_read_done_skipped_or_failed_goes_on_to_translate_and_one_stopped_ends_the_press() {
        let (_dir, session, pages) = chapter(2);
        written(&session, pages[0]);
        let host = Arc::new(Host::new());
        let held: Arc<dyn RunHost> = host.clone();
        let stops = host.pipeline.stops();
        let ends = [
            (Answer::Done, Outcome::Done),
            (Answer::Skipped, Outcome::Done),
            (Answer::Failed, Outcome::Done),
            (Answer::Stopped, Outcome::Stopped),
        ];
        for (answer, outcome) in ends {
            *host.answer.lock() = answer;
            assert_eq!(
                read_step(&held, &session, 7, pages.clone(), stops).await,
                outcome,
                "{answer:?}"
            );
        }
        assert_eq!(host.reads(), vec![(7, pages.clone(), stops); 4]);
        // A Stop since the press reached the read stops it, unread.
        host.pipeline.stop();
        assert_eq!(
            read_step(&held, &session, 7, pages.clone(), stops).await,
            Outcome::Stopped
        );
        // Checked again just before it reads: pages without text, or a
        // chapter read meanwhile, skip it, unread, and the press goes on.
        let stops = host.pipeline.stops();
        assert_eq!(
            read_step(&held, &session, 7, pages[1..].to_vec(), stops).await,
            Outcome::Done
        );
        briefed(&session, "Hina meets Sensei.");
        assert_eq!(
            read_step(&held, &session, 7, pages.clone(), stops).await,
            Outcome::Done
        );
        // A side file that doesn't read skips it too: the Translation run
        // reads it and fails, saying why.
        let path = session.lock().path().to_owned();
        std::fs::write(side_file(&path), "{").expect("a broken side file");
        assert_eq!(
            read_step(&held, &session, 7, pages.clone(), stops).await,
            Outcome::Done
        );
        assert_eq!(host.reads().len(), 4, "none of those was read");
    }

    #[tokio::test]
    async fn a_press_that_reads_first_takes_its_place_reading_then_translates() {
        let _registry = REGISTRY.lock().await;
        for answer in [Answer::Done, Answer::Skipped, Answer::Failed] {
            let (_dir, session, pages) = chapter(2);
            // Text to read on the first page; the second, which it
            // translates, has none, so its Translation run needs no model.
            written(&session, pages[0]);
            let host = Arc::new(Host::new());
            *host.answer.lock() = answer;
            let held: Arc<dyn RunHost> = host.clone();
            let target = RunTarget::Pages {
                pages: vec![pages[1]],
            };
            let (plan, following) = start(
                held,
                Arc::clone(&session),
                &target,
                &[RunStage::Read, RunStage::Translate],
                unlinked,
                &Handle::current(),
            )
            .expect("the plan");
            assert_eq!(plan.groups, [reading(&pages), translation(&pages[1..])]);
            assert!(plan.run.is_none() && plan.reading, "{plan:?}");
            following
                .expect("a follower")
                .await
                .expect("followed to the end");
            // The whole chapter read, then the Translation run, whatever
            // the read answered, announced as the last of the press's
            // groups, which the read never is.
            assert_eq!(host.reads(), [(plan.group, pages.clone(), 0)]);
            assert_eq!(host.asked(), [vec![Stage::Translate]], "{answer:?}");
            assert_eq!(host.queued(), [Target::Pages(pages[1..].to_vec())]);
            assert_eq!(host.announced(), [(plan.group, 1)]);
            assert!(GOING.lock().is_none());
        }
    }

    #[tokio::test]
    async fn a_press_reads_between_its_runs_and_a_stop_while_it_reads_ends_it() {
        let _registry = REGISTRY.lock().await;
        let (_dir, session, pages) = chapter(2);
        written(&session, pages[0]);
        let (came, mut coming) = tokio::sync::mpsc::unbounded_channel();
        let released = Arc::new(tokio::sync::Notify::new());
        let host = Arc::new(Host {
            holding: Some(Holding {
                came,
                released: Arc::clone(&released),
            }),
            ..Host::new()
        });
        let held: Arc<dyn RunHost> = host.clone();
        let target = RunTarget::Pages {
            pages: vec![pages[1]],
        };
        let stages = [RunStage::Detect, RunStage::Read, RunStage::Translate];
        let (plan, following) = start(
            held,
            Arc::clone(&session),
            &target,
            &stages,
            unlinked,
            &Handle::current(),
        )
        .expect("the plan");
        assert_eq!(
            plan.groups,
            [
                detection(&pages[1..]),
                reading(&pages),
                translation(&pages[1..])
            ]
        );
        assert!(plan.run.is_some() && !plan.reading, "{plan:?}");
        coming.recv().await.expect("the read");
        // While it reads, a call answers the group reading, with no run.
        let project = session.lock().id();
        let going = answer(GOING.lock().as_ref(), project).expect("the group");
        assert!(going.run.is_none() && going.reading, "{going:?}");
        host.pipeline.stop();
        released.notify_one();
        following
            .expect("a follower")
            .await
            .expect("followed to the end");
        assert_eq!(host.reads(), [(plan.group, pages.clone(), 0)]);
        assert_eq!(host.asked(), [vec![Stage::Detect]], "no Translation run");
        assert_eq!(host.announced(), [(plan.group, 0)]);
        assert!(GOING.lock().is_none());
    }

    #[tokio::test]
    async fn a_press_leaves_the_pages_a_run_gave_up_on_out_of_its_read_and_translation() {
        let _registry = REGISTRY.lock().await;
        let (_dir, session, pages) = chapter(3);
        let host = Host::new();
        let group = MADE.fetch_add(1, Ordering::Relaxed) + 1;
        *GOING.lock() = Some(Going {
            plan: RunPlan {
                group,
                groups: vec![detection(&pages), reading(&pages), translation(&pages)],
                run: Some(RunId(0)),
                reading: false,
            },
            project: session.lock().id(),
            stops: host.pipeline.stops(),
            narrows: false,
            failed: BTreeSet::new(),
            lettering: LetteringSettings::default(),
        });
        let problems = Outcome::DoneWithProblems {
            failures: vec![PageFailure {
                page: pages[1],
                stage: Stage::Detect,
                reason: "the model didn't answer".to_owned(),
            }],
        };
        let kept = vec![pages[0], pages[2]];
        let stops = host.pipeline.stops();
        assert_eq!(
            advance(&host, &session, group, 0, &problems).expect("the read"),
            Some(Now::Read(1, kept.clone(), stops))
        );
        let now = || {
            GOING
                .lock()
                .as_ref()
                .map(|going| (going.plan.run, going.plan.reading))
        };
        assert_eq!(now(), Some((None, true)));
        assert!(host.queued().is_empty() && host.announced().is_empty());
        let Some(Now::Run(at, run)) =
            advance(&host, &session, group, 1, &Outcome::Done).expect("the Translation run")
        else {
            panic!("no run queued");
        };
        assert_eq!(at, 2);
        assert_eq!(now(), Some((Some(run), false)));
        assert_eq!(host.queued(), [Target::Pages(kept)]);
        assert_eq!(host.announced(), [(group, 2)]);
        let outcome = ended(&host.pipeline, run).await.expect("the run's end");
        let next = advance(&host, &session, group, 2, &outcome).expect("the end");
        assert!(next.is_none() && GOING.lock().is_none());
    }

    #[tokio::test]
    async fn a_press_is_refused_while_read_chapter_reads_by_hand() {
        let _registry = REGISTRY.lock().await;
        let (_dir, session, _) = chapter(1);
        let press = || {
            start(
                Arc::new(Host::new()),
                Arc::clone(&session),
                &RunTarget::Project {},
                &run_stages(&EVERY, false),
                unlinked,
                &Handle::current(),
            )
        };
        let read = jobs::start(JobKind::ReadChapter).expect("a read");
        let error = press().expect_err("refused while it reads");
        assert!(error.to_string().contains("Analyze chapter"), "{error}");
        drop(read);
        let (plan, following) = press().expect("a plan");
        assert!(plan.groups.is_empty() && following.is_none(), "{plan:?}");
    }

    #[tokio::test]
    async fn a_press_that_would_read_is_refused_while_pages_still_load() {
        let _registry = REGISTRY.lock().await;
        let (_dir, session, pages) = chapter(2);
        // Text to read on the first page; the second, which it translates,
        // has none, so its Translation run needs no model.
        written(&session, pages[0]);
        let host = Arc::new(Host::new());
        host.loading.store(true, Ordering::Relaxed);
        let target = RunTarget::Pages {
            pages: vec![pages[1]],
        };
        let press = |stages: &[RunStage]| {
            let held: Arc<dyn RunHost> = host.clone();
            start(
                held,
                Arc::clone(&session),
                &target,
                stages,
                unlinked,
                &Handle::current(),
            )
        };
        let reads = [RunStage::Read, RunStage::Translate];
        let error = press(&reads).expect_err("refused while pages load");
        assert_eq!(
            error.to_string(),
            "Analyze chapter can't start while pages are still loading into this chapter; wait until they're in, or run without Analyze chapter."
        );
        assert!(GOING.lock().is_none(), "it took no place");
        assert!(host.queued().is_empty() && host.reads().is_empty());
        // A press that wouldn't read goes: one without Read chapter, and one
        // on a chapter with a brief.
        let (plan, following) = press(&[RunStage::Translate]).expect("the plan");
        assert_eq!(plan.groups, [translation(&pages[1..])]);
        following
            .expect("a follower")
            .await
            .expect("followed to the end");
        briefed(&session, "Hina meets Sensei.");
        let (plan, following) = press(&reads).expect("the plan");
        assert_eq!(plan.groups, [translation(&pages[1..])]);
        following
            .expect("a follower")
            .await
            .expect("followed to the end");
        assert!(host.reads().is_empty(), "nothing read");
        // Once the pages are in, the chapter is read.
        briefed(&session, " ");
        host.loading.store(false, Ordering::Relaxed);
        let (plan, following) = press(&reads).expect("the plan");
        assert_eq!(plan.groups, [reading(&pages), translation(&pages[1..])]);
        following
            .expect("a follower")
            .await
            .expect("followed to the end");
        assert_eq!(host.reads(), [(plan.group, pages.clone(), 0)]);
    }
}
