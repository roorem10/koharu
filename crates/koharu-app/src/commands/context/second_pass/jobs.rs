use anyhow::{Context as _, Result, ensure};
use koharu_pipeline::{Operation, Stage, StopToken};
use koharu_scene::ProjectId;
use tauri::{AppHandle, Manager as _};
use tauri_runtime_cef::CefRuntime;

use crate::commands::{
    ChannelExt as _, Error,
    processing::{Job, JobChannel, JobId, JobState, Processing},
    project::CurrentProject,
    runs::{BatchState, RunSettings},
};

use super::{SecondPassScope, run_second_pass};

pub(crate) async fn begin_job(
    handle: &AppHandle<CefRuntime>,
) -> Result<(JobId, StopToken, ProjectId)> {
    let settings = RunSettings::load()?;
    let (requested_project, project_name) = {
        let projects = handle.state::<CurrentProject>();
        let projects = projects.project.lock().await;
        let project = projects.as_ref().context("no project is open")?;
        (project.snapshot().project_id(), project.name.clone())
    };
    let id = JobId::new();
    let stop = StopToken::default();
    let processing = handle.state::<Processing>();
    {
        let mut stops = processing.stops.lock();
        ensure!(
            !handle.state::<BatchState>().active(),
            "a chapter batch is running"
        );
        ensure!(stops.is_empty(), "another process is already running");
        stops.insert(id, stop.clone());
    }
    let job = Job {
        id,
        state: JobState::Running,
        completed: 0,
        total: 0,
        page: None,
        stage: Some(Stage::Translation),
        model: None,
        error: None,
        retry: None,
        failures: Vec::new(),
        project: Some(project_name),
        operation: Some(Operation::Only {
            stage: Stage::Translation,
        }),
        finish_sound: settings.finish_sound,
    };
    processing.jobs.lock().insert(id, job.clone());
    handle.state::<JobChannel>().channel.publish(job);
    Ok((id, stop, requested_project))
}

pub(crate) fn finish_job(
    handle: &AppHandle<CefRuntime>,
    id: JobId,
    stop: &StopToken,
    error: Option<String>,
) {
    let processing = handle.state::<Processing>();
    let job = processing.jobs.lock().get_mut(&id).map(|job| {
        job.state = if stop.stopped() {
            JobState::Stopped
        } else if error.is_some() {
            JobState::Failed
        } else {
            JobState::Finished
        };
        job.error = error;
        job.retry = None;
        job.clone()
    });
    processing.stops.lock().remove(&id);
    if let Some(job) = job {
        handle.state::<JobChannel>().channel.publish(job);
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn start_second_pass(
    scope: SecondPassScope,
    handle: AppHandle<CefRuntime>,
) -> std::result::Result<JobId, Error> {
    let (id, stop, requested_project) = begin_job(&handle).await?;
    drop(tokio::spawn(async move {
        let result =
            run_second_pass(&handle, &scope, requested_project, &stop, Some(id)).await;
        finish_job(
            &handle,
            id,
            &stop,
            result.err().map(|error| format!("{error:#}")),
        );
    }));
    Ok(id)
}
