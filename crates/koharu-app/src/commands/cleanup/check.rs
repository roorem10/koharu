use anyhow::{Context as _, Result, ensure};
use koharu_pipeline::{Operation, Scope};
use koharu_scene::EntityId;
use tauri::{AppHandle, Manager as _, State};
use tauri_runtime_cef::CefRuntime;

use crate::commands::{
    Error,
    processing::{JobChannel, JobId, Processing, start_job},
    project::CurrentProject,
};

#[tauri::command]
#[specta::specta]
pub(crate) async fn check_cleanup(
    pages: Option<Vec<EntityId>>,
    handle: AppHandle<CefRuntime>,
) -> Result<JobId, Error> {
    Ok(start(handle, pages).await?)
}

/// Shared by the page button, agent, and a future automatic recipe. `None` checks
/// the whole project; explicit pages are processed in project order.
pub(crate) async fn start(
    handle: AppHandle<CefRuntime>,
    pages: Option<Vec<EntityId>>,
) -> Result<JobId> {
    {
        let projects = handle.state::<CurrentProject>();
        let projects = projects.project.lock().await;
        let snapshot = projects.as_ref().context("no project is open")?.snapshot();
        if let Some(pages) = &pages {
            ensure!(!pages.is_empty(), "select at least one page to check");
            for page in pages {
                snapshot.page(*page)?;
            }
        }
    }
    start_job(
        handle.clone(),
        pages.map_or(Scope::Project, Scope::Pages),
        Operation::CheckCleanup,
        false,
        &handle.state::<CurrentProject>(),
        &handle.state::<Processing>(),
        &handle.state::<JobChannel>(),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_cleanup_check_results(
    project: State<'_, CurrentProject>,
) -> Result<Vec<koharu_pipeline::cleanup_check::CleanupCheckResult>, Error> {
    let projects = project.project.lock().await;
    let snapshot = projects.as_ref().context("no project is open")?.snapshot();
    Ok(koharu_pipeline::cleanup_check::results(&snapshot)?)
}
