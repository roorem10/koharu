//! Targeted reading, translation and cleanup for text boxes.

mod geometry;
mod operations;
pub(crate) mod settings;
#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, time::Duration};

use anyhow::{Context as _, Result, ensure};
use image::GrayImage;
use koharu_desktop::Desktop;
use koharu_pipeline::{Bounds, Operation, Scope, Stage};
use koharu_scene::{Commit, EntityId, ProjectId, Snapshot};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager as _, State};
use tauri_runtime_cef::CefRuntime;

use super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    processing::{JobChannel, JobId, JobState, Processing, start_job},
    project::CurrentProject,
};
use geometry::{rectangle, text_mask, union_mask};
use operations::{box_geometry, page_of, redo_info, unique_layers};
use settings::{OcrBoxMode, OcrBoxSettings};

#[derive(Clone, Copy, Debug, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RedoAction {
    Read,
    Translate,
    Clean,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Type)]
pub struct RedoInfo {
    pub user_sources: u32,
    pub user_translations: u32,
}

fn ensure_idle(handle: &AppHandle<CefRuntime>) -> Result<()> {
    ensure!(
        handle.state::<Processing>().stops.lock().is_empty(),
        "another process is already running"
    );
    ensure!(
        !handle.state::<super::runs::BatchState>().active(),
        "a chapter batch is running"
    );
    Ok(())
}

async fn synchronize(
    handle: &AppHandle<CefRuntime>,
    commit: &Commit,
    page: Option<EntityId>,
) -> Result<()> {
    let desktop = handle.state::<Desktop>();
    desktop.synchronize(&commit.snapshot, page, commit).await?;
    handle
        .state::<CanvasChannel>()
        .channel
        .publish(desktop.canvas_state());
    Ok(())
}

async fn check_project(handle: &AppHandle<CefRuntime>, expected: ProjectId) -> Result<()> {
    let projects = handle.state::<CurrentProject>();
    let projects = projects.project.lock().await;
    ensure!(
        projects
            .as_ref()
            .is_some_and(|project| project.snapshot().project_id() == expected),
        "the project changed during processing"
    );
    Ok(())
}

// start_job owns the admission lane. Wait for its terminal state before submitting
// another mask; otherwise the second call would overwrite the single pending mask.
async fn wait_job(handle: &AppHandle<CefRuntime>, id: JobId, expected: ProjectId) -> Result<bool> {
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    loop {
        interval.tick().await;
        check_project(handle, expected).await?;
        let job = handle
            .state::<Processing>()
            .jobs
            .lock()
            .get(&id)
            .cloned()
            .context("processing job disappeared")?;
        match job.state {
            JobState::Running => {}
            JobState::Finished => return Ok(true),
            JobState::Stopped => return Ok(false),
            JobState::Failed | JobState::FinishedWithProblems => anyhow::bail!(
                "{}",
                job.error
                    .unwrap_or_else(|| "the job failed; check the activity list".into())
            ),
        }
    }
}

async fn run_text(
    handle: &AppHandle<CefRuntime>,
    layers: Vec<EntityId>,
    operation: Operation,
) -> Result<JobId> {
    start_job(
        handle.clone(),
        Scope::Entities(layers),
        operation,
        false,
        &handle.state::<CurrentProject>(),
        &handle.state::<Processing>(),
        &handle.state::<JobChannel>(),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn add_ocr_box(
    handle: AppHandle<CefRuntime>,
    page: EntityId,
    rect: Bounds,
    mode: OcrBoxMode,
) -> Result<EntityId, Error> {
    ensure_idle(&handle)?;
    let styles = super::styles::load_text_styles()?.read()?.clone();
    let (commit, layer, active, rect) = {
        let projects = handle.state::<CurrentProject>();
        let mut projects = projects.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let size = project.snapshot().page(page)?.page()?;
        let rect = geometry::clip_rect(rect, size.width, size.height)?;
        let (commit, layer) = project.create_ocr_box(page, rect, &styles).await?;
        project.record_commit(&commit);
        (commit, layer, project.active_page(), rect)
    };
    let expected = commit.snapshot.project_id();
    synchronize(&handle, &commit, active).await?;
    let mask = if mode == OcrBoxMode::ReadTranslateClean {
        let text = text_mask(&commit.snapshot, page).await?;
        let size = commit.snapshot.page(page)?.page()?;
        Some(
            tokio_rayon::spawn(move || {
                union_mask(
                    size.width.ceil() as u32,
                    size.height.ceil() as u32,
                    text.as_ref(),
                    &[rectangle(rect)],
                    0,
                )
            })
            .await?,
        )
    } else {
        None
    };
    check_project(&handle, expected).await?;
    let operation = if mode == OcrBoxMode::Read {
        Operation::Only { stage: Stage::Ocr }
    } else {
        Operation::Stages {
            stages: vec![Stage::Ocr, Stage::Translation],
        }
    };
    let job = run_text(&handle, vec![layer], operation).await?;
    if let Some(mask) = mask
        && wait_job(&handle, job, expected).await?
        && mask.as_raw().iter().any(|value| *value != 0)
    {
        super::cleanup::clean_mask(handle, page, mask).await?;
    }
    Ok(layer)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_ocr_redo_info(
    layers: Vec<EntityId>,
    project: State<'_, CurrentProject>,
) -> Result<RedoInfo, Error> {
    if layers.is_empty() {
        return Ok(RedoInfo::default());
    }
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    Ok(redo_info(&project.snapshot(), &layers)?)
}

async fn cleanup_masks(
    snapshot: &Snapshot,
    layers: &[EntityId],
) -> Result<Vec<(EntityId, GrayImage)>> {
    let mut groups = BTreeMap::<EntityId, Vec<koharu_scene::Geometry>>::new();
    for layer in unique_layers(snapshot, layers)? {
        groups
            .entry(page_of(snapshot, layer)?)
            .or_default()
            .push(box_geometry(snapshot, layer, true)?);
    }
    let mut masks = Vec::new();
    // Preserve project page order, independent of UUID ordering.
    for page in snapshot.pages() {
        let Some(polygons) = groups.remove(&page.id()) else {
            continue;
        };
        let size = page.page()?;
        let text = text_mask(snapshot, page.id()).await?;
        let mask = tokio_rayon::spawn(move || {
            union_mask(
                size.width.ceil() as u32,
                size.height.ceil() as u32,
                text.as_ref(),
                &polygons,
                4,
            )
        })
        .await?;
        if mask.as_raw().iter().any(|value| *value != 0) {
            masks.push((page.id(), mask));
        }
    }
    Ok(masks)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn redo_ocr_boxes(
    handle: AppHandle<CefRuntime>,
    layers: Vec<EntityId>,
    action: RedoAction,
    overwrite: bool,
) -> Result<Vec<JobId>, Error> {
    ensure_idle(&handle)?;
    let (snapshot, commit, active, layers) = {
        let projects = handle.state::<CurrentProject>();
        let mut projects = projects.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let layers = unique_layers(&project.snapshot(), &layers)?;
        let commit = project.clear_redo_edits(&layers, action, overwrite).await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (project.snapshot(), commit, project.active_page(), layers)
    };
    if let Some(commit) = &commit {
        synchronize(&handle, commit, active).await?;
    }
    let expected = snapshot.project_id();
    if matches!(action, RedoAction::Clean) {
        let masks = cleanup_masks(&snapshot, &layers).await?;
        if masks.is_empty() {
            return Err(anyhow::anyhow!("the selected boxes have no area to clean").into());
        }
        let mut jobs = Vec::new();
        let count = masks.len();
        for (index, (page, mask)) in masks.into_iter().enumerate() {
            check_project(&handle, expected).await?;
            let job = super::cleanup::clean_mask(handle.clone(), page, mask).await?;
            jobs.push(job);
            if index + 1 < count && !wait_job(&handle, job, expected).await? {
                break;
            }
        }
        return Ok(jobs);
    }
    check_project(&handle, expected).await?;
    Ok(vec![
        run_text(
            &handle,
            layers,
            Operation::Only {
                stage: match action {
                    RedoAction::Read => Stage::Ocr,
                    RedoAction::Translate => Stage::Translation,
                    RedoAction::Clean => unreachable!(),
                },
            },
        )
        .await?,
    ])
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn add_sfx_subtitle(
    handle: AppHandle<CefRuntime>,
    layer: EntityId,
) -> Result<EntityId, Error> {
    ensure_idle(&handle)?;
    let settings = OcrBoxSettings::load()?;
    let styles = super::styles::load_text_styles()?.read()?.clone();
    let (commit, added, translate, active) = {
        let projects = handle.state::<CurrentProject>();
        let mut projects = projects.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let (commit, added, translate) = project
            .create_sfx_subtitle(layer, &settings, &styles)
            .await?;
        project.record_commit(&commit);
        (commit, added, translate, project.active_page())
    };
    synchronize(&handle, &commit, active).await?;
    if translate {
        check_project(&handle, commit.snapshot.project_id()).await?;
        run_text(
            &handle,
            vec![added],
            Operation::Only {
                stage: Stage::Translation,
            },
        )
        .await?;
    }
    Ok(added)
}
