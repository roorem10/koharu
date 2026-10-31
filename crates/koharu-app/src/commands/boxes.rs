//! Join and split text boxes that share a bubble.

pub(crate) mod deletion;
mod operations;

use anyhow::Context as _;
use koharu_desktop::Desktop;
use koharu_pipeline::{BoxSettings, Operation, Scope, Stage};
use koharu_scene::{Commit, EntityId};
use schemars::JsonSchema;
use serde::Deserialize;
use tauri::{AppHandle, State};
use tauri_runtime_cef::CefRuntime;

use super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    processing::{JobChannel, JobId, Processing, start_job},
    project::CurrentProject,
};
pub use operations::BoxActionInfo;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct FindSplitBubblesArgs {
    pub page: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct JoinBubbleBoxesArgs {
    pub layer: String,
}

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

async fn synchronize(
    desktop: &Desktop,
    canvas: &CanvasChannel,
    commit: Option<&Commit>,
    page: Option<EntityId>,
) -> anyhow::Result<()> {
    if let Some(commit) = commit {
        desktop.synchronize(&commit.snapshot, page, commit).await?;
        canvas.channel.publish(desktop.canvas_state());
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_box_actions(
    layer: EntityId,
    project: State<'_, CurrentProject>,
) -> Result<BoxActionInfo, Error> {
    let project = project.project.lock().await;
    Ok(project
        .as_ref()
        .context("no project is open")?
        .box_action_info(layer)?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn join_bubble_boxes(
    layer: EntityId,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<EntityId, Error> {
    let (commit, kept, page) = {
        let mut project = project.project.lock().await;
        let project = project.as_mut().context("no project is open")?;
        let (commit, kept) = project.join_bubble_boxes(layer).await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, kept, project.active_page())
    };
    synchronize(&desktop, &canvas_channel, commit.as_ref(), page).await?;
    Ok(kept)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn join_page_bubbles(
    page: EntityId,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<u32, Error> {
    let (commit, count, active_page) = {
        let mut project = project.project.lock().await;
        let project = project.as_mut().context("no project is open")?;
        let (commit, count) = project.join_page_bubbles(page).await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, count, project.active_page())
    };
    synchronize(&desktop, &canvas_channel, commit.as_ref(), active_page).await?;
    Ok(count)
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn split_box(
    layer: EntityId,
    handle: AppHandle<CefRuntime>,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    processing: State<'_, Processing>,
    job_channel: State<'_, JobChannel>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<Vec<EntityId>, Error> {
    if !processing.stops.lock().is_empty() {
        return Err(anyhow::anyhow!("another process is already running").into());
    }
    let (commit, layers, page) = {
        let mut project = project.project.lock().await;
        let project = project.as_mut().context("no project is open")?;
        let (commit, layers) = project.split_box(layer).await?;
        project.record_commit(&commit);
        (commit, layers, project.active_page())
    };
    synchronize(&desktop, &canvas_channel, Some(&commit), page).await?;
    start_job(
        handle,
        Scope::Entities(layers.to_vec()),
        Operation::Stages {
            stages: vec![Stage::Ocr, Stage::Translation],
        },
        false,
        &project,
        &processing,
        &job_channel,
    )
    .await?;
    Ok(layers.to_vec())
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
