use std::time::Duration;

use anyhow::{Context as _, Result, bail, ensure};
use koharu_agent::{Control, Invocation};
use koharu_scene::EntityId;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Manager as _};
use tauri_runtime_cef::CefRuntime;

use crate::commands::{
    cleanup::check,
    processing::{JobState, Processing},
    project::CurrentProject,
};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckCleanup {
    /// Page IDs in the open project. Omit or use null to check all pages.
    pages: Option<Vec<String>>,
}

pub(super) async fn run(
    handle: AppHandle<CefRuntime>,
    arguments: CheckCleanup,
    control: &Control,
) -> Result<Invocation> {
    ensure!(!control.is_cancelled(), "cleanup check was cancelled");
    let pages = arguments
        .pages
        .map(|pages| {
            pages
                .into_iter()
                .map(|page| serde_json::from_value::<EntityId>(json!(page)))
                .collect::<std::result::Result<Vec<_>, _>>()
        })
        .transpose()
        .context("invalid cleanup-check page ID")?;
    let expected = {
        let current = handle.state::<CurrentProject>();
        let current = current.project.lock().await;
        current
            .as_ref()
            .context("no project is open")?
            .snapshot()
            .project_id()
    };
    let id = check::start(handle.clone(), pages.clone()).await?;
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    let job = loop {
        interval.tick().await;
        let processing = handle.state::<Processing>();
        if control.is_cancelled()
            && let Some(stop) = processing.stops.lock().get(&id)
        {
            stop.stop();
        }
        let job = processing
            .jobs
            .lock()
            .get(&id)
            .cloned()
            .context("cleanup-check job is no longer available")?;
        match job.state {
            JobState::Running => {}
            JobState::Stopped => bail!("cleanup check was cancelled"),
            JobState::Failed => bail!(
                "{}",
                job.error.as_deref().unwrap_or("cleanup check failed")
            ),
            JobState::Finished | JobState::FinishedWithProblems => break job,
        }
    };
    let current = handle.state::<CurrentProject>();
    let current = current.project.lock().await;
    let snapshot = current.as_ref().context("no project is open")?.snapshot();
    ensure!(
        snapshot.project_id() == expected,
        "the project changed during cleanup checking"
    );
    let mut results = koharu_pipeline::cleanup_check::results(&snapshot)?;
    if let Some(pages) = &pages {
        results.retain(|result| pages.contains(&result.page));
    }
    Invocation::changed(json!({ "job": job, "results": results }))
}
