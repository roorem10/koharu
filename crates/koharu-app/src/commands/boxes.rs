//! Join and split text boxes that share a bubble.

pub(crate) mod deletion;

use anyhow::Context as _;
use koharu_pipeline::{BoxSettings, Operation, Scope, Stage};
use koharu_scene::EntityId;
use tauri::{AppHandle, State};
use tauri_runtime_cef::CefRuntime;

use super::{
    Error,
    processing::{JobChannel, JobId, Processing, start_job},
    project::CurrentProject,
};

#[tauri::command]
#[specta::specta]
pub(crate) fn get_box_settings() -> Result<BoxSettings, Error> {
    Ok(koharu_config::load::<BoxSettings>("boxes")?.read()?.clone())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn save_box_settings(settings: BoxSettings) -> Result<BoxSettings, Error> {
    let config = koharu_config::load::<BoxSettings>("boxes")?;
    *config.write()? = settings.clone();
    config.save()?;
    Ok(settings)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn redetect_page(
    page: EntityId,
    handle: AppHandle<CefRuntime>,
    project: State<'_, CurrentProject>,
    processing: State<'_, Processing>,
    job_channel: State<'_, JobChannel>,
) -> Result<JobId, Error> {
    {
        let project = project.project.lock().await;
        project
            .as_ref()
            .context("no project is open")?
            .snapshot()
            .page(page)?;
    }
    Ok(start_job(
        handle,
        Scope::Pages(vec![page]),
        Operation::Only {
            stage: Stage::Detection,
        },
        true,
        &project,
        &processing,
        &job_channel,
    )
    .await?)
}
