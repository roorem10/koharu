//! The chapter recipe owns ordering; each additional check adds one recipe entry.
pub(super) mod freshness;

use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc, time::Instant};

use anyhow::{Context as _, Result, ensure};
use koharu_pipeline::{Operation, Pipeline, Progress, Request, RunStatus, Scope, Stage};
use koharu_scene::{EntityId, ProjectId, Snapshot};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager as _};
use tauri_runtime_cef::CefRuntime;

use super::{
    ChannelExt as _, Error,
    processing::{JobChannel, JobId, Processing},
    project::CurrentProject,
    runs::RunSettings,
};

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Deserialize,
    Serialize,
    Type,
    schemars::JsonSchema,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QualityLevel {
    Fast,
    #[default]
    Balanced,
    Best,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Type, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AutoRunStepId {
    FixLevels,
    Detection,
    Ocr,
    ReadChapter,
    Translation,
    Cleanup,
    Typeset,
    Qc,
    Export,
}

impl AutoRunStepId {
    pub(crate) fn from_stage(stage: Stage) -> Self {
        match stage {
            Stage::Detection => Self::Detection,
            Stage::Ocr => Self::Ocr,
            Stage::Translation => Self::Translation,
            Stage::Inpainting => Self::Cleanup,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Type, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AutoRunSkipReason {
    Disabled,
    Quality,
    NoSeries,
    NoVision,
    UpToDate,
    NoSource,
    HandEdited,
    PrerequisiteFailed,
    ExportNotConfigured,
}

#[derive(Clone, Debug, Serialize, Type)]
pub(crate) struct AutoRunPlannedStep {
    pub id: AutoRunStepId,
    pub needed: bool,
    pub reason: Option<AutoRunSkipReason>,
}

#[derive(Clone, Debug, Serialize, Type)]
pub(crate) struct AutoRunPreview {
    pub quality: QualityLevel,
    pub steps: Vec<AutoRunPlannedStep>,
    pub pages: u32,
    pub can_read_images: bool,
    pub estimated_requests: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Type, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AutoRunStepStatus {
    Ran,
    Skipped,
    Failed,
}

#[derive(Clone, Debug, Serialize, Type)]
pub(crate) struct AutoRunStepResult {
    pub id: AutoRunStepId,
    pub status: AutoRunStepStatus,
    pub reason: Option<AutoRunSkipReason>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Type)]
pub(crate) struct AutoRunSummary {
    pub steps: Vec<AutoRunStepResult>,
    pub pages: u32,
    pub failures: u32,
    pub elapsed_seconds: f64,
}

#[derive(Clone, Debug, Default, Serialize, Type)]
pub(crate) struct AutoRunProgress {
    pub step: Option<AutoRunStepId>,
    pub summary: Option<AutoRunSummary>,
}

struct Facts {
    needed: BTreeSet<AutoRunStepId>,
    levels: bool,
    export: bool,
    linked: bool,
    vision: bool,
    manual_brief: bool,
    read_current: bool,
    source: bool,
}

type StepFuture<'a> = Pin<Box<dyn Future<Output = Result<bool>> + Send + 'a>>;
type NativeStep = for<'a, 'b> fn(&'a mut Run<'b>) -> StepFuture<'a>;

#[derive(Clone, Copy)]
enum Work {
    Pipeline(Stage),
    Native(NativeStep),
}

struct Step {
    id: AutoRunStepId,
    minimum: QualityLevel,
    needed: fn(&Facts) -> Option<AutoRunSkipReason>,
    work: Work,
}

fn missing(needed: bool) -> Option<AutoRunSkipReason> {
    (!needed).then_some(AutoRunSkipReason::UpToDate)
}

fn reading_needed(facts: &Facts) -> Option<AutoRunSkipReason> {
    if !facts.linked {
        Some(AutoRunSkipReason::NoSeries)
    } else if !facts.vision {
        Some(AutoRunSkipReason::NoVision)
    } else if facts.manual_brief {
        Some(AutoRunSkipReason::HandEdited)
    } else if !facts.source {
        Some(AutoRunSkipReason::NoSource)
    } else {
        missing(!facts.read_current)
    }
}

// Adjacent pipeline entries share a pass, preserving the scheduler's page overlap.
// Native entries are barriers, including Read chapter after every page has OCR.
const RECIPE: &[Step] = &[
    Step {
        id: AutoRunStepId::FixLevels,
        minimum: QualityLevel::Fast,
        needed: |f| if !f.levels {
            Some(AutoRunSkipReason::Disabled)
        } else {
            missing(f.needed.contains(&AutoRunStepId::FixLevels))
        },
        work: Work::Native(levels),
    },
    Step {
        id: AutoRunStepId::Detection,
        minimum: QualityLevel::Fast,
        needed: |f| missing(f.needed.contains(&AutoRunStepId::Detection)),
        work: Work::Pipeline(Stage::Detection),
    },
    Step {
        id: AutoRunStepId::Ocr,
        minimum: QualityLevel::Fast,
        needed: |f| missing(f.needed.contains(&AutoRunStepId::Ocr)),
        work: Work::Pipeline(Stage::Ocr),
    },
    Step {
        id: AutoRunStepId::ReadChapter,
        minimum: QualityLevel::Balanced,
        needed: reading_needed,
        work: Work::Native(read_chapter),
    },
    Step {
        id: AutoRunStepId::Translation,
        minimum: QualityLevel::Fast,
        needed: |f| missing(f.needed.contains(&AutoRunStepId::Translation)),
        work: Work::Pipeline(Stage::Translation),
    },
    Step {
        id: AutoRunStepId::Cleanup,
        minimum: QualityLevel::Fast,
        needed: |f| missing(f.needed.contains(&AutoRunStepId::Cleanup)),
        work: Work::Pipeline(Stage::Inpainting),
    },
    Step {
        id: AutoRunStepId::Typeset,
        minimum: QualityLevel::Fast,
        needed: |f| missing(f.needed.contains(&AutoRunStepId::Typeset)),
        work: Work::Native(typeset),
    },
    Step {
        id: AutoRunStepId::Qc,
        minimum: QualityLevel::Fast,
        needed: |f| missing(f.needed.contains(&AutoRunStepId::Qc)),
        work: Work::Native(qc),
    },
];

fn decision(step: &Step, quality: QualityLevel, facts: &Facts) -> Option<AutoRunSkipReason> {
    if quality < step.minimum {
        Some(AutoRunSkipReason::Quality)
    } else {
        (step.needed)(facts)
    }
}

fn plan(
    quality: QualityLevel,
    facts: &Facts,
    pages: u32,
    translation_requests: u32,
    chunk: u8,
) -> AutoRunPreview {
    let mut steps: Vec<_> = RECIPE
        .iter()
        .map(|step| {
            let reason = decision(step, quality, facts);
            AutoRunPlannedStep {
                id: step.id,
                needed: reason.is_none(),
                reason,
            }
        })
        .collect();
    let reads = steps
        .iter()
        .any(|step| step.id == AutoRunStepId::ReadChapter && step.needed);
    // This checkout only saves the export filename pattern, not a format/destination.
    // Never open a save dialog from an unattended chapter job.
    steps.push(AutoRunPlannedStep {
        id: AutoRunStepId::Export,
        needed: false,
        reason: Some(if facts.export {
            AutoRunSkipReason::ExportNotConfigured
        } else {
            AutoRunSkipReason::Disabled
        }),
    });
    AutoRunPreview {
        quality,
        steps,
        pages,
        can_read_images: facts.vision,
        estimated_requests: translation_requests
            + if reads {
                pages.div_ceil(u32::from(chunk.max(1)))
            } else {
                0
            },
    }
}

pub(crate) fn batch_preview(pages: u32, quality: QualityLevel) -> Result<AutoRunPreview> {
    let settings = RunSettings::load()?;
    let model = super::context::reading::get_reading_model()
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let facts = Facts {
        needed: RECIPE.iter().map(|step| step.id).collect(),
        levels: settings.auto_run_fix_levels,
        export: settings.auto_run_export,
        linked: false,
        vision: model.vision,
        manual_brief: false,
        read_current: false,
        source: true,
    };
    Ok(plan(quality, &facts, pages, pages, 1))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn preview_auto_run(
    scope: Scope,
    quality: QualityLevel,
    handle: AppHandle<CefRuntime>,
) -> std::result::Result<AutoRunPreview, Error> {
    let settings = RunSettings::load()?;
    let (snapshot, context) = current(&handle, None).await?;
    let pages = scoped_pages(&snapshot, &scope)?;
    let pipeline = handle.state::<Pipeline>();
    let mut request = Request {
        scope: Scope::Pages(pages.clone()),
        resume: true,
        ..Default::default()
    };
    request.translation_hints = Arc::new(super::context::translation_hints(&context, &snapshot)?);
    freshness::refresh(&snapshot, &context, &mut request)?;
    let (mut facts, requests) = facts(&snapshot, &context, &pipeline, &request, &settings, true)?;
    if super::qc::auto_needed(&handle, &pages, snapshot.project_id()).await?
        || !facts.needed.is_empty()
    {
        facts.needed.insert(AutoRunStepId::Qc);
    }
    let chunk = super::context::get_context_settings()?.pages_per_request;
    Ok(plan(quality, &facts, pages.len() as u32, requests, chunk))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn auto_run(
    scope: Scope,
    quality: QualityLevel,
    handle: AppHandle<CefRuntime>,
) -> std::result::Result<JobId, Error> {
    Ok(super::processing::start_auto_job(handle, scope, quality, false).await?)
}

fn scoped_pages(snapshot: &Snapshot, scope: &Scope) -> Result<Vec<EntityId>> {
    ensure!(
        matches!(scope, Scope::Project | Scope::Pages(_)),
        "Auto run requires a chapter or pages"
    );
    if let Scope::Pages(ids) = scope {
        for &page in ids {
            snapshot.page(page)?;
        }
    }
    let pages: Vec<_> = snapshot
        .pages()
        .map(|page| page.id())
        .filter(|id| match scope {
            Scope::Pages(ids) => ids.contains(id),
            _ => true,
        })
        .collect();
    ensure!(!pages.is_empty(), "there are no pages to run");
    Ok(pages)
}

async fn current(
    handle: &AppHandle<CefRuntime>,
    expected: Option<ProjectId>,
) -> Result<(Snapshot, booharu_context::ProjectContext)> {
    let state = handle.state::<CurrentProject>();
    let project = state.project.lock().await;
    let project = project.as_ref().context("no project is open")?;
    let snapshot = project.snapshot();
    ensure!(
        expected.is_none_or(|expected| expected == snapshot.project_id()),
        "the project changed during Auto run"
    );
    Ok((snapshot, project.load_context()?))
}

fn facts(
    snapshot: &Snapshot,
    context: &booharu_context::ProjectContext,
    pipeline: &Pipeline,
    request: &Request,
    settings: &RunSettings,
    predict: bool,
) -> Result<(Facts, u32)> {
    let pages = scoped_pages(snapshot, &request.scope)?;
    let pending = pipeline.pending(snapshot, request)?;
    let mut needed: BTreeSet<_> = pending
        .iter()
        .map(|(stage, _)| AutoRunStepId::from_stage(*stage))
        .collect();
    let mut translation_pages: BTreeSet<_> = pending
        .iter()
        .filter_map(|(stage, page)| (*stage == Stage::Translation).then_some(*page))
        .collect();
    // Before OCR the exact number of translatable pages is unknown; use an upper estimate.
    if predict {
        for (stage, page) in &pending {
            if matches!(stage, Stage::Detection | Stage::Ocr) {
                translation_pages.insert(*page);
                needed.extend([
                    AutoRunStepId::Ocr,
                    AutoRunStepId::Translation,
                    AutoRunStepId::Typeset,
                ]);
                if *stage == Stage::Detection {
                    needed.insert(AutoRunStepId::Cleanup);
                }
            }
        }
    }
    for &page in &pages {
        if super::raw_prep::auto_needed(snapshot, page)? {
            needed.insert(AutoRunStepId::FixLevels);
        }
    }
    if needed.contains(&AutoRunStepId::Translation)
        || super::rich_text::auto_typeset_needed(
            snapshot,
            &pages,
            freshness::typeset_changed(snapshot, context, &pages)?,
        )?
    {
        needed.insert(AutoRunStepId::Typeset);
    }
    let model = super::context::reading::get_reading_model()
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let source = super::context::reading::has_source_lines(snapshot, &request.scope)?
        || (predict && needed.contains(&AutoRunStepId::Ocr));
    Ok((
        Facts {
            needed,
            levels: settings.auto_run_fix_levels,
            export: settings.auto_run_export,
            linked: super::context::linked_series(context)?.is_some(),
            vision: model.vision,
            manual_brief: context.brief.as_ref().is_some_and(|brief| brief.edited),
            read_current: freshness::reading_current(snapshot, context, &pages)?,
            source,
        },
        translation_pages.len() as u32,
    ))
}

struct Run<'a> {
    handle: &'a AppHandle<CefRuntime>,
    project: ProjectId,
    pages: Vec<EntityId>,
    request: Request,
}

fn publish(
    handle: &AppHandle<CefRuntime>,
    id: JobId,
    step: Option<AutoRunStepId>,
    summary: Option<AutoRunSummary>,
) {
    let state = handle.state::<Processing>();
    let job = state.jobs.lock().get_mut(&id).map(|job| {
        job.auto_run = Some(AutoRunProgress { step, summary });
        job.model = None;
        job.stage = None;
        job.retry = None;
        job.clone()
    });
    if let Some(job) = job {
        handle.state::<JobChannel>().channel.publish(job);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute(
    handle: &AppHandle<CefRuntime>,
    pipeline: &Pipeline,
    project: ProjectId,
    request: Request,
    committer: &mut super::processing::PipelineCommitter,
    id: JobId,
    quality: QualityLevel,
    settings: &RunSettings,
) -> Result<koharu_pipeline::Report> {
    let started = Instant::now();
    let (snapshot, _) = current(handle, Some(project)).await?;
    let pages = scoped_pages(&snapshot, &request.scope)?;
    let base = snapshot.revision();
    let mut run = Run {
        handle,
        project,
        pages,
        request,
    };
    run.request.scope = Scope::Pages(run.pages.clone());
    run.request.resume = true;
    let mut results = Vec::new();
    let mut failed = BTreeSet::new();
    let mut position = 0;
    let outcome: Result<()> = async {
        while position < RECIPE.len() && !run.request.stop.stopped() {
            let step = &RECIPE[position];
            let (snapshot, context) = current(handle, Some(project)).await?;
            run.request.translation_hints =
                Arc::new(super::context::translation_hints(&context, &snapshot)?);
            freshness::refresh(&snapshot, &context, &mut run.request)?;
            let (mut facts, _) = facts(&snapshot, &context, pipeline, &run.request, settings, true)?;
            if super::qc::auto_needed(handle, &run.pages, project).await? {
                facts.needed.insert(AutoRunStepId::Qc);
            }
            if let Work::Pipeline(_) = step.work {
                let first = position;
                while position < RECIPE.len()
                    && matches!(RECIPE[position].work, Work::Pipeline(_))
                {
                    position += 1;
                }
                let stages: Vec<_> = RECIPE[first..position]
                    .iter()
                    .filter(|entry| decision(entry, quality, &facts).is_none())
                    .filter_map(|entry| match entry.work {
                        Work::Pipeline(stage) => Some(stage),
                        _ => None,
                    })
                    .collect();
                if stages.is_empty() {
                    for entry in &RECIPE[first..position] {
                        results.push(skipped(
                            entry.id,
                            decision(entry, quality, &facts)
                                .unwrap_or(AutoRunSkipReason::UpToDate),
                        ));
                    }
                    continue;
                }
                let eligible: Vec<_> = run
                    .pages
                    .iter()
                    .copied()
                    .filter(|page| !failed.contains(page))
                    .collect();
                if eligible.is_empty() {
                    for entry in &RECIPE[first..position] {
                        results.push(skipped(entry.id, AutoRunSkipReason::PrerequisiteFailed));
                    }
                    continue;
                }
                publish(handle, id, Some(step.id), None);
                let seen = Arc::new(parking_lot::Mutex::new(BTreeSet::new()));
                let seen_progress = seen.clone();
                let previous = run.request.progress.clone();
                let mut pass = run.request.clone();
                pass.operation = Operation::Stages { stages };
                pass.scope = Scope::Pages(eligible);
                pass.progress = Some(Arc::new(move |event| {
                    if let Progress::Finished { stage, .. } = &event {
                        seen_progress.lock().insert(*stage);
                    }
                    if let Some(progress) = &previous {
                        progress(event);
                    }
                }));
                let report =
                    super::processing::run_pass(handle, pipeline, project, pass, committer).await;
                match report {
                    Ok(report) => {
                        failed.extend(report.failures.iter().map(|failure| failure.page));
                        for entry in &RECIPE[first..position] {
                            let Work::Pipeline(stage) = entry.work else {
                                unreachable!()
                            };
                            let failure = report.failures.iter().find(|failure| failure.stage == stage);
                            results.push(if let Some(failure) = failure {
                                AutoRunStepResult {
                                    id: entry.id,
                                    status: AutoRunStepStatus::Failed,
                                    reason: None,
                                    error: Some(failure.error.clone()),
                                }
                            } else if seen.lock().contains(&stage) {
                                ran(entry.id)
                            } else {
                                skipped(entry.id, AutoRunSkipReason::UpToDate)
                            });
                        }
                        if report.status == RunStatus::Stopped {
                            run.request.stop.stop();
                        }
                    }
                    Err(error) => {
                        for entry in &RECIPE[first..position] {
                            results.push(AutoRunStepResult {
                                id: entry.id,
                                status: AutoRunStepStatus::Failed,
                                reason: None,
                                error: Some(format!("{error:#}")),
                            });
                        }
                        return Err(error);
                    }
                }
                continue;
            }
            position += 1;
            if step.id == AutoRunStepId::ReadChapter && !failed.is_empty() {
                results.push(skipped(step.id, AutoRunSkipReason::PrerequisiteFailed));
                continue;
            }
            if let Some(reason) = decision(step, quality, &facts) {
                // Typesetting also runs as part of committing each translation.
                if step.id == AutoRunStepId::Typeset
                    && results.iter().any(|result| {
                        result.id == AutoRunStepId::Translation
                            && result.status == AutoRunStepStatus::Ran
                    })
                {
                    results.push(ran(step.id));
                } else {
                    results.push(skipped(step.id, reason));
                }
                continue;
            }
            publish(handle, id, Some(step.id), None);
            let Work::Native(work) = step.work else {
                unreachable!()
            };
            let mut attempt = 0;
            loop {
                match work(&mut run).await {
                    Ok(changed) => {
                        results.push(if run.request.stop.stopped() {
                            skipped(step.id, AutoRunSkipReason::PrerequisiteFailed)
                        } else if changed {
                            ran(step.id)
                        } else {
                            skipped(step.id, AutoRunSkipReason::UpToDate)
                        });
                        break;
                    }
                    Err(error) if attempt < settings.retries && !run.request.stop.stopped() => {
                        attempt += 1;
                        tracing::warn!(%error, attempt, "retrying Auto run step");
                        let processing = handle.state::<Processing>();
                        let job = processing.jobs.lock().get_mut(&id).map(|job| {
                            job.retry = Some(super::processing::RetryProgress {
                                attempt: attempt + 1,
                                attempts: settings.retries + 1,
                            });
                            job.clone()
                        });
                        if let Some(job) = job {
                            handle.state::<JobChannel>().channel.publish(job);
                        }
                    }
                    Err(error) => {
                        if let Some(record) = handle
                            .state::<Processing>()
                            .runs
                            .lock()
                            .get_mut(&id)
                        {
                            record.failed.extend(run.pages.iter().copied());
                            record.failed.sort_unstable();
                            record.failed.dedup();
                        }
                        results.push(AutoRunStepResult {
                            id: step.id,
                            status: AutoRunStepStatus::Failed,
                            reason: None,
                            error: Some(format!("{error:#}")),
                        });
                        if settings.on_page_failure == koharu_pipeline::OnPageFailure::StopRun {
                            return Err(error);
                        }
                        break;
                    }
                }
            }
        }
        Ok(())
    }
    .await;
    for step in &RECIPE[position..] {
        results.push(skipped(step.id, AutoRunSkipReason::PrerequisiteFailed));
    }
    results.push(skipped(
        AutoRunStepId::Export,
        if settings.auto_run_export {
            AutoRunSkipReason::ExportNotConfigured
        } else {
            AutoRunSkipReason::Disabled
        },
    ));
    let pipeline_step = |step: AutoRunStepId| {
        matches!(
            step,
            AutoRunStepId::Detection
                | AutoRunStepId::Ocr
                | AutoRunStepId::Translation
                | AutoRunStepId::Cleanup
        )
    };
    let page_failures = handle
        .state::<Processing>()
        .jobs
        .lock()
        .get(&id)
        .map_or(0, |job| job.failures.len() as u32);
    let native_failures = results
        .iter()
        .filter(|step| step.status == AutoRunStepStatus::Failed && !pipeline_step(step.id))
        .count() as u32;
    let pipeline_failed = results
        .iter()
        .any(|step| step.status == AutoRunStepStatus::Failed && pipeline_step(step.id));
    let failures = (native_failures + page_failures.max(u32::from(pipeline_failed)))
        .max(u32::from(outcome.is_err()));
    if outcome.is_err()
        && let Some(record) = handle.state::<Processing>().runs.lock().get_mut(&id)
        && record.failed.is_empty()
    {
        record.failed.extend(run.pages.iter().copied());
    }
    publish(
        handle,
        id,
        None,
        Some(AutoRunSummary {
            steps: results,
            pages: run.pages.len() as u32,
            failures,
            elapsed_seconds: started.elapsed().as_secs_f64(),
        }),
    );
    outcome?;
    let (snapshot, _) = current(handle, Some(project)).await?;
    Ok(koharu_pipeline::Report {
        status: if run.request.stop.stopped() {
            RunStatus::Stopped
        } else if failures > 0 {
            RunStatus::CompletedWithProblems
        } else {
            RunStatus::Completed
        },
        base,
        final_revision: snapshot.revision(),
        completed: 0,
        total: 0,
        elapsed: started.elapsed(),
        failures: vec![],
    })
}

fn ran(id: AutoRunStepId) -> AutoRunStepResult {
    AutoRunStepResult {
        id,
        status: AutoRunStepStatus::Ran,
        reason: None,
        error: None,
    }
}

fn skipped(id: AutoRunStepId, reason: AutoRunSkipReason) -> AutoRunStepResult {
    AutoRunStepResult {
        id,
        status: AutoRunStepStatus::Skipped,
        reason: Some(reason),
        error: None,
    }
}

fn levels<'a>(run: &'a mut Run<'_>) -> StepFuture<'a> {
    Box::pin(async move {
        Ok(super::raw_prep::auto_prepare(run.handle, &run.pages, run.project, &run.request.stop)
            .await?
            .changed
            > 0)
    })
}

fn read_chapter<'a>(run: &'a mut Run<'_>) -> StepFuture<'a> {
    Box::pin(async move {
        let model = super::context::reading::get_reading_model()
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        super::context::reading::read(
            run.handle,
            &run.request.scope,
            model.selection,
            run.project,
            &run.request.stop,
        )
        .await?;
        if run.request.stop.stopped() {
            return Ok(false);
        }
        freshness::remember_reading(run.handle, run.project, &run.pages).await?;
        Ok(true)
    })
}

fn typeset<'a>(run: &'a mut Run<'_>) -> StepFuture<'a> {
    Box::pin(async move {
        let state = run.handle.state::<CurrentProject>();
        let (commit, page) = {
            let mut projects = state.project.lock().await;
            let project = projects.as_mut().context("no project is open")?;
            ensure!(
                project.snapshot().project_id() == run.project,
                "the project changed during typesetting"
            );
            if run.request.stop.stopped() {
                return Ok(false);
            }
            let refresh = freshness::typeset_changed(
                &project.snapshot(),
                &project.load_context()?,
                &run.pages,
            )?;
            let commit = super::rich_text::auto_typeset(project, &run.pages, refresh).await?;
            if let Some(commit) = &commit {
                project.record_commit(commit);
            }
            freshness::remember_typeset(project, &run.pages, true)?;
            (commit, project.active_page())
        };
        if let Some(commit) = commit {
            let desktop = run.handle.state::<koharu_desktop::Desktop>();
            desktop.synchronize(&commit.snapshot, page, &commit).await?;
            run.handle
                .state::<super::canvas::CanvasChannel>()
                .channel
                .publish(desktop.canvas_state());
            return Ok(true);
        }
        Ok(false)
    })
}

fn qc<'a>(run: &'a mut Run<'_>) -> StepFuture<'a> {
    Box::pin(async move {
        super::qc::auto_check(run.handle, &run.pages, run.project, &run.request.stop).await
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            needed: BTreeSet::new(),
            levels: false,
            export: false,
            linked: true,
            vision: true,
            manual_brief: false,
            read_current: false,
            source: true,
        }
    }

    #[test]
    fn quality_gates_reading_but_never_replaces_manual_work() {
        let mut facts = facts();
        let read = RECIPE
            .iter()
            .find(|step| step.id == AutoRunStepId::ReadChapter)
            .unwrap();
        assert_eq!(
            decision(read, QualityLevel::Fast, &facts),
            Some(AutoRunSkipReason::Quality)
        );
        assert_eq!(decision(read, QualityLevel::Balanced, &facts), None);
        facts.manual_brief = true;
        assert_eq!(
            decision(read, QualityLevel::Best, &facts),
            Some(AutoRunSkipReason::HandEdited)
        );
        facts.manual_brief = false;
        facts.read_current = true;
        assert_eq!(
            decision(read, QualityLevel::Balanced, &facts),
            Some(AutoRunSkipReason::UpToDate)
        );
    }

    #[test]
    fn missing_reading_prerequisites_have_explicit_reasons() {
        let mut facts = facts();
        facts.linked = false;
        assert_eq!(reading_needed(&facts), Some(AutoRunSkipReason::NoSeries));
        facts.linked = true;
        facts.vision = false;
        assert_eq!(reading_needed(&facts), Some(AutoRunSkipReason::NoVision));
        facts.vision = true;
        facts.source = false;
        assert_eq!(reading_needed(&facts), Some(AutoRunSkipReason::NoSource));
    }

    #[test]
    fn estimates_count_translation_and_chunked_reads_only() {
        let facts = facts();
        assert_eq!(
            plan(QualityLevel::Fast, &facts, 41, 10, 20).estimated_requests,
            10
        );
        assert_eq!(
            plan(QualityLevel::Balanced, &facts, 41, 10, 20).estimated_requests,
            13
        );
        assert_eq!(
            plan(QualityLevel::Best, &facts, 41, 10, 20).estimated_requests,
            13
        );
    }
}
