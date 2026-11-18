//! Going on: the groups of a plan, one after another, each queued as an
//! ordinary run of Koharu's that follows the run policy, whoever asked for
//! it, the Run button through the desktop command or the agent through its
//! tool, both `booharu_run`, which reach the app through a `RunHost`.
//!
//! One group goes at a time, as one project is open at a time: a call while
//! one goes in the open project answers that one. The next group is queued
//! once the run before it ends Done or DoneWithProblems, on the pages of it
//! the project still has; a run that ends Failed or Stopped ends the group,
//! and so does the project closing or another opening, which stops the run
//! going. Each run is followed through `Pipeline::history`, which keeps
//! every event, where the broadcast the interface hears can drop some.

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
use parking_lot::Mutex;

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

/// A group going on: its plan, with the run going now, and the project it
/// works on.
struct Going {
    plan: RunPlan,
    project: ProjectId,
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
    let starting = Arc::clone(&host);
    let (plan, first) = tokio::task::spawn_blocking(move || {
        let (plan, run) = start(&*starting, &session, &target, &stages)?;
        anyhow::Ok((plan, run.map(|run| (session, run))))
    })
    .await??;
    if let Some((session, run)) = first {
        tokio::spawn(follow(host, session, plan.group, run));
    }
    Ok(plan)
}

/// `carry_on`'s work, on the thread that calls it: the plan, and the first
/// run it queued, which a follower then follows.
fn start(
    host: &dyn RunHost,
    session: &Arc<Mutex<Session>>,
    target: &RunTarget,
    stages: &[Stage],
) -> anyhow::Result<(RunPlan, Option<RunId>)> {
    let project = session.lock().id();
    let mut going = GOING.lock();
    if let Some(plan) = answer(going.as_ref(), project) {
        return Ok((plan, None));
    }
    let groups = groups(&session.lock(), target, stages)?;
    let group = MADE.fetch_add(1, Ordering::Relaxed) + 1;
    let Some(first) = groups.first() else {
        let plan = RunPlan {
            group,
            groups,
            run: None,
        };
        return Ok((plan, None));
    };
    let request = request(first_target(target, stages, first), &first.stages);
    let run = host
        .queue(session, request)?
        .context("the project closed")?;
    let plan = RunPlan {
        group,
        groups,
        run: Some(run),
    };
    host.announce(&plan, 0, run);
    *going = Some(Going {
        plan: plan.clone(),
        project,
    });
    Ok((plan, Some(run)))
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

/// Follows `group` from `run`, its first: once each run ends, queues the
/// group's next run, or ends the group.
async fn follow(host: Arc<dyn RunHost>, session: Arc<Mutex<Session>>, group: u64, mut run: RunId) {
    let mut index = 0;
    loop {
        let outcome = match ended(host.pipeline(), run).await {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::warn!("couldn't follow run group {group}: {error:#}");
                free(group);
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
                free(group);
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
/// went to another project's group. A run that can't be queued ends it too.
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
    use koharu_pipeline::PageFailure;
    use koharu_scene::Uuid;

    use super::*;

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
}
