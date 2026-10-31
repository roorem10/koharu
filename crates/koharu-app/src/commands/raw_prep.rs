//! Levels replace only page source assets; existing cleanup is never invalidated.
mod images;
mod operations;
mod settings;
#[cfg(test)]
mod tests;

use anyhow::{Context as _, Result, ensure};
use koharu_desktop::Desktop;
use koharu_scene::{AssetRole, EntityId};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager as _, State};
use tauri_runtime_cef::CefRuntime;

use super::{
    ChannelExt as _, Error, canvas::CanvasChannel, processing::Processing, project::CurrentProject,
};
pub(crate) use settings::{RawPrepOptions, RawPrepSettings};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PrepSkipReason {
    Cleanup,
    MissingSource,
    NoOriginal,
    Unchanged,
}

#[derive(Debug, Serialize, Type)]
pub(crate) struct PrepSkipped {
    pub page: EntityId,
    pub label: String,
    pub reason: PrepSkipReason,
}

#[derive(Debug, Default, Serialize, Type)]
pub(crate) struct PrepResult {
    pub changed: u32,
    pub skipped: Vec<PrepSkipped>,
}

#[derive(Debug, Serialize, Type)]
pub(crate) struct PrepPageStatus {
    pub page: EntityId,
    pub has_original: bool,
    pub has_cleanup: bool,
}

#[derive(Debug, Serialize, Type)]
pub(crate) struct PrepPreview {
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub black: u8,
    pub white: u8,
}

#[tauri::command]
#[specta::specta]
pub(crate) fn get_raw_prep_settings() -> std::result::Result<RawPrepSettings, Error> {
    Ok(settings::load()?)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn save_raw_prep_settings(
    settings: RawPrepSettings,
) -> std::result::Result<RawPrepSettings, Error> {
    settings.options.validate()?;
    let config = koharu_config::load::<RawPrepSettings>("raw_prep")?;
    let mut write = config.write()?;
    *write = settings.clone();
    write.save()?;
    Ok(settings)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_prep_page_status(
    pages: Vec<EntityId>,
    project: State<'_, CurrentProject>,
) -> std::result::Result<Vec<PrepPageStatus>, Error> {
    let project = project.project.lock().await;
    let project = project.as_ref().context("no project is open")?;
    Ok(operations::statuses(&project.snapshot(), pages)?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn preview_prep(
    page: EntityId,
    options: RawPrepOptions,
    project: State<'_, CurrentProject>,
) -> std::result::Result<PrepPreview, Error> {
    options.validate()?;
    let snapshot = project
        .project
        .lock()
        .await
        .as_ref()
        .context("no project is open")?
        .snapshot();
    snapshot.page(page)?;
    let source = snapshot
        .asset(page, &AssetRole::new("source")?)?
        .context("page has no source image")?;
    let bytes = snapshot.read_blob(source.blob).await?;
    Ok(tokio_rayon::spawn(move || images::preview(&bytes, options)).await?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn prepare_pages(
    pages: Vec<EntityId>,
    options: RawPrepOptions,
    handle: AppHandle<CefRuntime>,
) -> std::result::Result<PrepResult, Error> {
    Ok(apply_pages(&handle, pages, Some(options)).await?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn restore_original_pages(
    pages: Vec<EntityId>,
    handle: AppHandle<CefRuntime>,
) -> std::result::Result<PrepResult, Error> {
    Ok(apply_pages(&handle, pages, None).await?)
}

async fn apply_pages(
    handle: &AppHandle<CefRuntime>,
    pages: Vec<EntityId>,
    options: Option<RawPrepOptions>,
) -> Result<PrepResult> {
    let processing = handle.state::<Processing>();
    ensure!(
        processing.stops.lock().is_empty(),
        "page sources cannot change while processing is running"
    );
    let current = handle.state::<CurrentProject>();
    let snapshot = current
        .project
        .lock()
        .await
        .as_ref()
        .context("no project is open")?
        .snapshot();
    let (patch, result) = match options {
        Some(options) => operations::prepare_patch(&snapshot, pages, options).await?,
        None => operations::restore_patch(&snapshot, pages).await?,
    };
    if patch.is_empty() {
        return Ok(result);
    }
    let (commit, page) = {
        let mut project = current.project.lock().await;
        let project = project.as_mut().context("no project is open")?;
        ensure!(
            project.snapshot().project_id() == snapshot.project_id()
                && project.revision() == snapshot.revision(),
            "the project changed while preparing pages; try again"
        );
        ensure!(
            processing.stops.lock().is_empty(),
            "page sources cannot change while processing is running"
        );
        let commit = project.commit(patch).await?;
        project.record_commit(&commit);
        (commit, project.active_page())
    };
    let desktop = handle.state::<Desktop>();
    desktop.synchronize(&commit.snapshot, page, &commit).await?;
    handle
        .state::<CanvasChannel>()
        .channel
        .publish(desktop.canvas_state());
    Ok(result)
}

/// Imported pages are prepared before the import command completes.
pub(crate) async fn after_import(handle: &AppHandle<CefRuntime>, pages: Vec<EntityId>) {
    let prepare = async {
        let settings = settings::load()?;
        if settings.fix_on_import && !pages.is_empty() {
            apply_pages(handle, pages, Some(settings.options)).await?;
        }
        Ok::<_, anyhow::Error>(())
    };
    if let Err(error) = prepare.await {
        tracing::warn!(%error, "could not apply page levels on import");
    }
}
