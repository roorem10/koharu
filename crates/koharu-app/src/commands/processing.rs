use std::{collections::HashMap, fmt, sync::Arc};

use anyhow::{Context as _, Result};
use koharu_pipeline::{Committer, Progress, RunStatus, StageOutput, StopToken};
use koharu_scene::Snapshot;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager as _, State, ipc::Channel};
use tauri_runtime_cef::CefRuntime;
use uuid::Uuid;

use super::{ChannelExt as _, Error, canvas::CanvasChannel, project::CurrentProject};
use koharu_desktop::Desktop;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Type)]
#[serde(transparent)]
pub struct JobId(Uuid);

impl JobId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct Job {
    pub id: JobId,
    pub state: JobState,
    #[specta(type = f64)]
    pub completed: usize,
    #[specta(type = f64)]
    pub total: usize,
    pub page: Option<koharu_scene::EntityId>,
    pub stage: Option<koharu_pipeline::Stage>,
    pub model: Option<String>,
    pub error: Option<String>,
    pub retry: Option<RetryProgress>,
    pub failures: Vec<koharu_pipeline::PageFailure>,
    pub project: Option<String>,
    pub operation: Option<koharu_pipeline::Operation>,
    pub finish_sound: bool,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct RetryProgress {
    pub attempt: u8,
    pub attempts: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Running,
    Finished,
    FinishedWithProblems,
    Failed,
    Stopped,
}

#[derive(Clone)]
pub(crate) struct RunRecord {
    pub(crate) project: koharu_scene::ProjectId,
    pub(crate) operation: koharu_pipeline::Operation,
    pub(crate) failed: Vec<koharu_scene::EntityId>,
}

#[derive(Default)]
pub(crate) struct Processing {
    pub(crate) stops: Mutex<HashMap<JobId, StopToken>>,
    pub(crate) jobs: Mutex<HashMap<JobId, Job>>,
    pub(crate) runs: Mutex<HashMap<JobId, RunRecord>>,
    pub(crate) inpainting_mask: Mutex<Option<koharu_pipeline::InpaintingMask>>,
}

#[derive(Default)]
pub(crate) struct JobChannel {
    pub(crate) channel: Mutex<Option<Channel<Job>>>,
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn process(
    handle: AppHandle<CefRuntime>,
    scope: koharu_pipeline::Scope,
    operation: koharu_pipeline::Operation,
    project: State<'_, CurrentProject>,
    processing: State<'_, Processing>,
    job_channel: State<'_, JobChannel>,
) -> std::result::Result<JobId, Error> {
    Ok(start_job(
        handle,
        scope,
        operation,
        false,
        &project,
        &processing,
        &job_channel,
    )
    .await?)
}

/// Starts a pipeline job, as the Run menu does. `redetect` makes detection run again on pages
/// that already have detected text (WO-20's "Detect again"). WO-14 owns the body; the
/// signature is shared.
pub(crate) async fn start_job(
    handle: AppHandle<CefRuntime>,
    scope: koharu_pipeline::Scope,
    operation: koharu_pipeline::Operation,
    redetect: bool,
    project: &CurrentProject,
    processing: &Processing,
    job_channel: &JobChannel,
) -> Result<JobId> {
    start_job_inner(
        handle,
        scope,
        operation,
        redetect,
        project,
        processing,
        job_channel,
        false,
    )
    .await
}

pub(crate) async fn start_batch_job(
    handle: AppHandle<CefRuntime>,
    operation: koharu_pipeline::Operation,
) -> Result<JobId> {
    start_job_inner(
        handle.clone(),
        koharu_pipeline::Scope::Project,
        operation,
        false,
        &handle.state::<CurrentProject>(),
        &handle.state::<Processing>(),
        &handle.state::<JobChannel>(),
        true,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn start_job_inner(
    handle: AppHandle<CefRuntime>,
    scope: koharu_pipeline::Scope,
    operation: koharu_pipeline::Operation,
    redetect: bool,
    project: &CurrentProject,
    processing: &Processing,
    job_channel: &JobChannel,
    batch: bool,
) -> Result<JobId> {
    anyhow::ensure!(
        batch || !handle.state::<super::runs::BatchState>().active(),
        "a chapter batch is running"
    );
    let settings = super::runs::RunSettings::load()?;
    let (translation_hints, run_project, project_name) = {
        let project = project.project.lock().await;
        let project = project.as_ref().context("no project is open")?;
        let snapshot = project.snapshot();
        let hints = super::context::translation_hints(&project.load_context()?, &snapshot)?;
        (Arc::new(hints), snapshot.project_id(), project.name.clone())
    };
    let id = JobId::new();
    let stop = StopToken::default();
    {
        let mut stops = processing.stops.lock();
        anyhow::ensure!(
            batch || !handle.state::<super::runs::BatchState>().active(),
            "a chapter batch is running"
        );
        if !stops.is_empty() {
            anyhow::bail!("another process is already running");
        }
        stops.insert(id, stop.clone());
    }
    let job = Job {
        id,
        state: JobState::Running,
        completed: 0,
        total: 0,
        page: None,
        stage: None,
        model: None,
        error: None,
        retry: None,
        failures: Vec::new(),
        project: Some(project_name),
        operation: Some(operation.clone()),
        finish_sound: settings.finish_sound && !batch,
    };
    processing.runs.lock().insert(
        id,
        RunRecord {
            project: run_project,
            operation: operation.clone(),
            failed: Vec::new(),
        },
    );
    processing.jobs.lock().insert(id, job.clone());
    job_channel.channel.publish(job);

    let pipeline = handle.state::<koharu_pipeline::Pipeline>().inner().clone();
    let task_handle = handle.clone();
    let inpainting_mask = processing.inpainting_mask.lock().take();
    drop(tokio::spawn(async move {
        let progress = Arc::new(Mutex::new((0_usize, 0_usize)));
        let progress_handle = task_handle.clone();
        let mut request = koharu_pipeline::Request {
            operation,
            scope,
            stop: stop.clone(),
            progress: None,
            inpainting_mask,
            translation_hints,
            redetect,
            policy: settings.policy(),
        };
        request.progress = Some(Arc::new(move |event| {
            let update = match event {
                Progress::Started { pages, stages } => {
                    let mut progress = progress.lock();
                    *progress = (0, pages.len().saturating_mul(stages.len()));
                    Some((0, progress.1, None, None, None))
                }
                Progress::Loading { page, stage, model } => {
                    let progress = progress.lock();
                    Some((progress.0, progress.1, Some(page), Some(stage), Some(model)))
                }
                Progress::Finished {
                    page, stage, model, ..
                } => {
                    let mut progress = progress.lock();
                    progress.0 = progress.0.saturating_add(1).min(progress.1);
                    Some((progress.0, progress.1, Some(page), Some(stage), Some(model)))
                }
                Progress::Skipped { page, stage } => {
                    let mut progress = progress.lock();
                    progress.0 = progress.0.saturating_add(1).min(progress.1);
                    Some((progress.0, progress.1, Some(page), Some(stage), None))
                }
                Progress::PageFailed { failure, settled } => {
                    if let Some(record) = progress_handle
                        .state::<Processing>()
                        .runs
                        .lock()
                        .get_mut(&id)
                    {
                        record.failed.push(failure.page);
                    }
                    let counts = {
                        let mut progress = progress.lock();
                        progress.0 = progress.0.saturating_add(settled).min(progress.1);
                        *progress
                    };
                    let processing = progress_handle.state::<Processing>();
                    let job = {
                        let mut jobs = processing.jobs.lock();
                        jobs.get_mut(&id).map(|job| {
                            job.failures.push(failure);
                            job.completed = counts.0;
                            job.total = counts.1;
                            job.clone()
                        })
                    };
                    if let Some(job) = job {
                        progress_handle.state::<JobChannel>().channel.publish(job);
                    }
                    None
                }
                Progress::Retrying {
                    page,
                    stage,
                    attempt,
                    attempts,
                } => {
                    let processing = progress_handle.state::<Processing>();
                    let job = {
                        let mut jobs = processing.jobs.lock();
                        jobs.get_mut(&id).map(|job| {
                            job.page = Some(page);
                            job.stage = Some(stage);
                            job.retry = Some(RetryProgress { attempt, attempts });
                            job.clone()
                        })
                    };
                    if let Some(job) = job {
                        progress_handle.state::<JobChannel>().channel.publish(job);
                    }
                    None
                }
                Progress::Running { .. } => None,
            };
            if let Some((completed, total, page, stage, model)) = update {
                let job = {
                    let processing = progress_handle.state::<Processing>();
                    let mut jobs = processing.jobs.lock();
                    jobs.get_mut(&id).map(|job| {
                        job.retry = None;
                        job.completed = completed;
                        job.total = total;
                        job.page = page;
                        job.stage = stage;
                        job.model = model;
                        job.clone()
                    })
                };
                if let Some(job) = job {
                    progress_handle.state::<JobChannel>().channel.publish(job);
                }
            }
        }));

        struct PipelineCommitter {
            handle: AppHandle<CefRuntime>,
            project: koharu_scene::ProjectId,
        }

        #[async_trait::async_trait]
        impl Committer for PipelineCommitter {
            async fn commit(&mut self, output: StageOutput) -> Result<Snapshot> {
                let (commit, page) = {
                    let projects = self.handle.state::<CurrentProject>();
                    let mut projects = projects.project.lock().await;
                    let project = projects.as_mut().context("no project is open")?;
                    anyhow::ensure!(
                        project.snapshot().project_id() == self.project,
                        "the project changed during processing"
                    );
                    let Some(mut commit) = project.commit_rebased(output.patch).await? else {
                        return Ok(project.snapshot());
                    };
                    project.record_commit(&commit);
                    if output.stage == koharu_pipeline::Stage::Translation {
                        // WO-11A: a new translation wipes word styles and resets
                        // the box style (decision 4).
                        if let Some(reset) =
                            super::rich_text::after_translation(project, &commit).await?
                        {
                            project.record_commit(&reset);
                            commit = reset;
                        }
                        super::context::refresh_flags(project)?;
                    }
                    let page = project.active_page();
                    (commit, page)
                };
                let snapshot = commit.snapshot.clone();
                let desktop = self.handle.state::<Desktop>();
                desktop.synchronize(&commit.snapshot, page, &commit).await?;
                let canvas = desktop.canvas_state();
                self.handle.state::<CanvasChannel>().channel.publish(canvas);
                Ok(snapshot)
            }
        }

        let mut committer = PipelineCommitter {
            handle: task_handle.clone(),
            project: run_project,
        };
        // A failed chapter read must not block translation; the page requests
        // simply go out without the new brief.
        if super::context::operation_translates(&request.operation)
            && let Err(error) =
                super::context::auto_read(&task_handle, &request.scope, &stop, id).await
        {
            tracing::warn!(%error, "automatic Read chapter failed");
        }
        let result = async {
            let snapshot = {
                let projects = task_handle.state::<CurrentProject>();
                let projects = projects.project.lock().await;
                let project = projects.as_ref().context("no project is open")?;
                let snapshot = project.snapshot();
                anyhow::ensure!(
                    snapshot.project_id() == run_project,
                    "the project changed during processing"
                );
                request.translation_hints = Arc::new(super::context::translation_hints(
                    &project.load_context()?,
                    &snapshot,
                )?);
                snapshot
            };
            pipeline
                .execute(snapshot, request, &mut committer)
                .await
                .map_err(anyhow::Error::from)
        }
        .await;
        let (stopped, error) = match result {
            Ok(report) => (report.status == RunStatus::Stopped, None),
            Err(error) => {
                tracing::error!(%error, "processing failed");
                (false, Some(format!("{error:#}")))
            }
        };
        let stopped = stopped || stop.stopped();
        task_handle.state::<Processing>().stops.lock().remove(&id);
        let job = task_handle
            .state::<Processing>()
            .jobs
            .lock()
            .get_mut(&id)
            .map(|job| {
                job.state = if stopped {
                    JobState::Stopped
                } else if error.is_some() {
                    JobState::Failed
                } else if job.failures.is_empty() {
                    JobState::Finished
                } else {
                    JobState::FinishedWithProblems
                };
                job.error = error;
                job.retry = None;
                job.clone()
            });
        if let Some(job) = job {
            task_handle.state::<JobChannel>().channel.publish(job);
        }
    }));
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn unload_models(
    pipeline: State<'_, koharu_pipeline::Pipeline>,
    processing: State<'_, Processing>,
) -> std::result::Result<u32, Error> {
    if !processing.stops.lock().is_empty() {
        return Err(
            anyhow::anyhow!("models cannot be unloaded while processing is running").into(),
        );
    }
    Ok(pipeline.unload_models().await?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn stop_job(
    job: JobId,
    processing: State<'_, Processing>,
) -> std::result::Result<(), Error> {
    let stops = processing.stops.lock();
    let stop = stops
        .get(&job)
        .with_context(|| format!("job {job} is not running"))?;
    stop.stop();
    Ok(())
}
