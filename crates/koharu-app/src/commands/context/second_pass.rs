//! A line review uses Explain's request and commits its wording and review record together.
mod hard_lines;
pub(crate) mod jobs;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use anyhow::{Context as _, Result, ensure};
use booharu_context::{GlossaryEntry, ProjectContext, Series};
use koharu_pipeline::{Pipeline, StopToken};
use koharu_scene::{
    AssetInput, AssetMetadata, AssetRole, Authored, EntityId, Generation, LanguageTag, Origin,
    Patch, ProducerId, ProjectId, Snapshot, Translation,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use tauri::{AppHandle, Manager as _, State};
use tauri_runtime_cef::CefRuntime;

use super::super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    processing::{JobChannel, JobId, Processing},
    project::{CurrentProject, Project},
};
use super::{line_request::LineAnswer, lines::Row};

const ROLE: &str = "booharu-second-pass";
const DISMISSED: &str = "second_pass_dismissed_suggestions";

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub(crate) enum SecondPassScope {
    Boxes { layers: Vec<EntityId> },
    Puns,
    Hard,
}

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub(crate) struct SecondPassResult {
    pub previous: String,
    pub best: String,
    pub alternatives: Vec<String>,
    pub reason: String,
}

#[derive(Default, Serialize)]
pub(crate) struct SecondPassReport {
    pub completed: u32,
    pub skipped: u32,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_second_pass(
    layer: EntityId,
    project: State<'_, CurrentProject>,
) -> std::result::Result<Option<SecondPassResult>, Error> {
    let snapshot = {
        let projects = project.project.lock().await;
        projects.as_ref().context("no project is open")?.snapshot()
    };
    Ok(result(&snapshot, layer).await?)
}

async fn result(snapshot: &Snapshot, layer: EntityId) -> Result<Option<SecondPassResult>> {
    let content = snapshot.text_layer(layer)?.content()?;
    let Some(asset) = snapshot.asset(content.id(), &AssetRole::new(ROLE)?)? else {
        return Ok(None);
    };
    let source = content
        .source()?
        .map(|source| source.text.value)
        .unwrap_or_default();
    if asset.metadata.attributes.get("source") != Some(&source) {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(
        &snapshot.read_blob(asset.blob).await?,
    )?))
}

/// Suggestions live with the result asset, so undoing a line also removes its suggestions.
/// Only the user's accept/dismiss choices live in the existing project sidecar.
pub(crate) fn merge_suggestions(snapshot: &Snapshot, context: &mut ProjectContext) -> Result<()> {
    let glossary = super::linked_series(context)?
        .map(|series| series.glossary)
        .unwrap_or_default();
    let mut contents = BTreeSet::new();
    for page in snapshot.pages() {
        for layer in page.text_layers()? {
            let content = layer.content()?;
            if !contents.insert(content.id()) {
                continue;
            }
            let Some(asset) = snapshot.asset(content.id(), &AssetRole::new(ROLE)?)? else {
                continue;
            };
            let source = content
                .source()?
                .map(|source| source.text.value)
                .unwrap_or_default();
            let translation = content
                .translation()?
                .map(|translation| translation.text.value)
                .unwrap_or_default();
            if asset.metadata.attributes.get("source") != Some(&source) {
                continue;
            }
            let Some(terms) = asset.metadata.attributes.get("suggestions") else {
                continue;
            };
            let terms: Vec<GlossaryEntry> = serde_json::from_str(terms)?;
            for term in terms {
                if !koharu_pipeline::term_occurs(&translation, &term.target)
                    || context
                        .other
                        .get(DISMISSED)
                        .and_then(|value| value.as_array())
                        .is_some_and(|ids| {
                            ids.iter().any(|id| id.as_str() == Some(term.id.as_str()))
                        })
                    || glossary
                        .iter()
                        .any(|entry| entry.source.trim() == term.source.trim())
                {
                    continue;
                }
                if let Some(pending) = context
                    .suggestions
                    .iter_mut()
                    .find(|entry| entry.source.trim() == term.source.trim())
                {
                    *pending = term;
                } else {
                    context.suggestions.push(term);
                }
            }
        }
    }
    Ok(())
}

pub(super) fn dismiss_suggestion(context: &mut ProjectContext, id: &str, source: Option<&str>) {
    context
        .suggestions
        .retain(|term| term.id != id && source != Some(term.source.as_str()));
    let ids = context.other.entry(DISMISSED).or_insert_with(|| json!([]));
    if let Some(ids) = ids.as_array_mut()
        && !ids.iter().any(|entry| entry.as_str() == Some(id))
    {
        ids.push(json!(id));
    }
}

fn targets(project: &Project, scope: &SecondPassScope) -> Result<Vec<EntityId>> {
    let snapshot = project.snapshot();
    let context = project.load_context()?;
    let series = super::linked_series(&context)?;
    let qc = super::super::qc::flagged_layers(project)?;
    if let SecondPassScope::Boxes { layers } = scope {
        ensure!(
            !layers.is_empty(),
            "choose at least one box for the second pass"
        );
        for layer in layers {
            snapshot.text_layer(*layer)?;
        }
    }
    Ok(super::lines::text_rows(&snapshot)?
        .into_iter()
        .filter(|row| {
            let hard = hard_lines::classify(
                row.layer,
                &row.source,
                &context,
                series.as_ref(),
                qc.contains(&row.layer),
            );
            match scope {
                SecondPassScope::Boxes { layers } => layers.contains(&row.layer),
                SecondPassScope::Puns => hard.pun,
                SecondPassScope::Hard => hard.is_hard(),
            }
        })
        .map(|row| row.layer)
        .collect())
}

fn eligible(row: &Row, scope: &SecondPassScope) -> bool {
    (!row.user || matches!(scope, SecondPassScope::Boxes { .. }))
        && !row.source.trim().is_empty()
}

/// Quality Best can call this inside its existing job; this function starts no second job.
/// Each response is committed immediately as one undo step, in project page order.
pub(crate) async fn run_second_pass(
    handle: &AppHandle<CefRuntime>,
    scope: &SecondPassScope,
    requested_project: ProjectId,
    stop: &StopToken,
    job: Option<JobId>,
) -> Result<SecondPassReport> {
    let mut translation = koharu_pipeline::PipelineConfig::load()?
        .read()?
        .translation
        .clone();
    if let Some(model) = super::settings::load()?.hard_line_model {
        translation.model = model;
    }
    ensure!(
        super::reading::can_read_images(&translation.model),
        "second pass needs a vision-capable language model"
    );
    translation.model.vision = true;
    translation.generation.vision = Some(true);
    let layers = {
        let projects = handle.state::<CurrentProject>();
        let projects = projects.project.lock().await;
        let project = projects.as_ref().context("no project is open")?;
        ensure!(
            project.snapshot().project_id() == requested_project,
            "project changed before second pass started"
        );
        super::refresh_flags(project)?;
        targets(project, scope)?
    };
    let mut report = SecondPassReport::default();
    publish_progress(handle, job, 0, layers.len(), None);
    let mut contents = BTreeSet::new();
    for (index, layer) in layers.iter().copied().enumerate() {
        if stop.stopped() {
            break;
        }
        let (snapshot, context) = {
            let projects = handle.state::<CurrentProject>();
            let projects = projects.project.lock().await;
            let project = projects.as_ref().context("no project is open")?;
            ensure!(
                project.snapshot().project_id() == requested_project,
                "project changed during second pass"
            );
            (project.snapshot(), project.load_context()?)
        };
        let row = super::lines::text_rows(&snapshot)?
            .into_iter()
            .find(|row| row.layer == layer);
        let Some(row) = row.filter(|row| eligible(row, scope)) else {
            report.skipped += 1;
            publish_progress(handle, job, index + 1, layers.len(), None);
            continue;
        };
        let content = snapshot.text_layer(layer)?.content()?.id();
        if !contents.insert(content) {
            report.skipped += 1;
            publish_progress(handle, job, index + 1, layers.len(), Some(row.page));
            continue;
        }
        let series = super::linked_series(&context)?;
        publish_progress(handle, job, index, layers.len(), Some(row.page));
        let request = super::line_request::request(&snapshot, &context, &row, &translation).await?;
        if stop.stopped() {
            break;
        }
        let answer = handle
            .state::<Pipeline>()
            .run_task(&translation.model, translation.generation, request)
            .await?;
        if stop.stopped() {
            break;
        }
        let answer = LineAnswer::parse(answer)?;
        let model = format!(
            "{}:{}",
            translation.model.provider,
            translation.model.model.as_deref().unwrap_or_default()
        );
        let patch = result_patch(
            &snapshot,
            &row,
            series.as_ref(),
            &answer,
            &LanguageTag::new(translation.target_language.tag())?,
            model,
        )?;
        // User word styling has a different owner from generated translation. Its
        // offset remap is a second patch grouped into the same undo step.
        let remap = snapshot.patch(|edit| {
            if row.user {
                Ok(())
            } else {
                super::super::rich_text::on_text_edit(edit, &snapshot, layer, &answer.best)
            }
        })?;
        let (commit, page) = {
            let projects = handle.state::<CurrentProject>();
            let mut projects = projects.project.lock().await;
            let project = projects.as_mut().context("no project is open")?;
            ensure!(
                project.snapshot().project_id() == requested_project,
                "project changed during second pass"
            );
            if stop.stopped() {
                break;
            }
            ensure!(
                project.load_context()? == context && super::linked_series(&context)? == series,
                "chapter context changed during second pass; run it again"
            );
            let Some(commit) = commit_result(project, patch, remap).await? else {
                report.skipped += 1;
                publish_progress(handle, job, index + 1, layers.len(), Some(row.page));
                continue;
            };
            report.completed += 1;
            super::refresh_flags(project)?;
            (commit, project.active_page())
        };
        let desktop = handle.state::<koharu_desktop::Desktop>();
        desktop.synchronize(&commit.snapshot, page, &commit).await?;
        handle
            .state::<CanvasChannel>()
            .channel
            .publish(desktop.canvas_state());
        publish_progress(handle, job, index + 1, layers.len(), Some(row.page));
    }
    Ok(report)
}

async fn commit_result(
    project: &mut Project,
    patch: Patch,
    remap: Patch,
) -> Result<Option<koharu_scene::Commit>> {
    let Some(mut commit) = project.commit_rebased(patch).await? else {
        return Ok(None);
    };
    let mut revisions = vec![commit.revision];
    if !remap.is_empty() {
        let remapped = match remap.rebase_on(&project.snapshot()) {
            Ok(remap) => project.commit(remap).await,
            Err(error) => Err(error.into()),
        };
        match remapped {
            Ok(remapped) => {
                revisions.push(remapped.revision);
                commit = remapped;
            }
            Err(error) => {
                project.record(revisions);
                return Err(error);
            }
        }
    }
    project.record(revisions);
    Ok(Some(commit))
}

fn result_patch(
    snapshot: &Snapshot,
    row: &Row,
    series: Option<&Series>,
    answer: &LineAnswer,
    language: &LanguageTag,
    model: String,
) -> Result<Patch> {
    let content = snapshot.text_layer(row.layer)?.content()?.id();
    let previous = snapshot.component::<Translation>(content)?;
    let manual = previous
        .as_ref()
        .is_some_and(|value| matches!(value.text.origin, Origin::User));
    let producer = match previous.map(|value| value.text.origin) {
        Some(Origin::Generated(generation)) => generation.producer,
        Some(Origin::User) | None => snapshot
            .asset(content, &AssetRole::new(ROLE)?)?
            .and_then(|asset| match asset.origin {
                Origin::Generated(generation) => Some(generation.producer),
                Origin::User => None,
            })
            .unwrap_or(ProducerId::new("dev.koharu.pipeline.translation")?),
    };
    let mut generation = Generation::new(producer);
    generation.model = Some(model);
    let result = SecondPassResult {
        previous: row.translation.clone(),
        best: answer.best.clone(),
        alternatives: answer.alternatives.clone(),
        reason: answer.explanation.clone(),
    };
    let mut suggestions: Vec<GlossaryEntry> = snapshot
        .asset(content, &AssetRole::new(ROLE)?)?
        .and_then(|asset| asset.metadata.attributes.get("suggestions").cloned())
        .map(|terms| serde_json::from_str(&terms))
        .transpose()?
        .unwrap_or_default();
    suggestions.retain(|term| {
        koharu_pipeline::term_occurs(&row.source, &term.source)
            && koharu_pipeline::term_occurs(&answer.best, &term.target)
            && !series
                .into_iter()
                .flat_map(|series| &series.glossary)
                .any(|entry| entry.source.trim() == term.source.trim())
    });
    for term in &answer.glossary {
        if !koharu_pipeline::term_occurs(&row.source, &term.source)
            || !koharu_pipeline::term_occurs(&answer.best, &term.target)
            || series
                .into_iter()
                .flat_map(|series| &series.glossary)
                .chain(&suggestions)
                .any(|entry| entry.source.trim() == term.source.trim())
        {
            continue;
        }
        suggestions.push(GlossaryEntry {
            id: EntityId::new().to_string(),
            source: term.source.trim().into(),
            target: term.target.trim().into(),
            note: term.note.clone(),
            kind: term.kind,
            locked: false,
        });
    }
    let metadata = AssetMetadata {
        width: None,
        height: None,
        attributes: [
            ("source".into(), row.source.clone()),
            ("suggestions".into(), serde_json::to_string(&suggestions)?),
        ]
        .into(),
    };
    let asset = AssetInput::new(serde_json::to_vec(&result)?, "application/json", metadata);
    let mut edit = snapshot.edit_as(generation.clone());
    edit.observe_subtree(row.page)?;
    if !manual {
        edit.set(
            content,
            &Translation {
                text: Authored::generated(answer.best.clone(), generation.clone()),
                language: Some(language.clone()),
            },
        )?;
    }
    edit.set_asset(content, &AssetRole::new(ROLE)?, asset)?;
    // Old emphasis hints describe different words and must not be consumed later.
    if !manual {
        for relation in snapshot.relations_to_as::<koharu_scene::Presents>(content) {
            let layer = relation.value().source;
            if snapshot
                .component::<koharu_pipeline::EmphasisHints>(layer)?
                .is_some_and(|hints| {
                    matches!(hints.origin, Origin::Generated(ref owner) if owner.producer == generation.producer)
                })
            {
                edit.remove::<koharu_pipeline::EmphasisHints>(layer)?;
            }
        }
    }
    Ok(edit.finish()?)
}

fn publish_progress(
    handle: &AppHandle<CefRuntime>,
    id: Option<JobId>,
    completed: usize,
    total: usize,
    page: Option<EntityId>,
) {
    let Some(id) = id else { return };
    let processing = handle.state::<Processing>();
    let job = processing.jobs.lock().get_mut(&id).map(|job| {
        job.completed = completed;
        job.total = total;
        job.page = page;
        job.clone()
    });
    if let Some(job) = job {
        handle.state::<JobChannel>().channel.publish(job);
    }
}
