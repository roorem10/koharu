//! Bounded QC repair using the ordinary pipeline, job cancellation and scene undo.
use std::{collections::BTreeSet, sync::Arc};

use anyhow::{Context as _, Result, ensure};
use booharu_context::{LabelSource, ProjectContext};
use koharu_desktop::Desktop;
use koharu_pipeline::{Committer, Operation, Scope, Stage, StageOutput, StopToken};
use koharu_scene::{
    EntityId, EntityOrigin, Geometry, Origin, Patch, ProjectId, Snapshot, SourceText, TextLayout,
    Translation, Typography, Visibility,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager as _, State};
use tauri_runtime_cef::CefRuntime;

use super::super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    processing::{Job, JobChannel, JobId, JobState, Processing},
    project::{CurrentProject, Project},
};
use super::{QcCheck, QcFinding, QcScope, plan::next_fix};

const HISTORY: &str = "dev.booharu.qc.auto-fixes";
const ATTEMPTS: &str = "qc_fix_attempts";
const SUMMARY: &str = "qc_fix_summary";

#[cfg(test)]
#[path = "fix_tests.rs"]
mod tests;

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub(crate) enum QcFixScope {
    Project,
    Pages { pages: Vec<EntityId> },
    Boxes { layers: Vec<EntityId> },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Type)]
pub(crate) struct QcFixSummary {
    pub fixes_made: u32,
    pub items_left: u32,
    /// Pipeline OCR/translation requests, including failed attempts.
    pub requests_used: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QcFixAction {
    TightenLayout,
    ShortenTranslation,
    TranslateMissing,
    TranslateLeftover,
    TranslateGlossary,
    ApplyCategoryStyle,
    ReadAgain,
    HideEmpty,
}

impl QcFixAction {
    fn stage(self) -> Option<Stage> {
        match self {
            Self::ReadAgain => Some(Stage::Ocr),
            Self::ShortenTranslation
            | Self::TranslateMissing
            | Self::TranslateLeftover
            | Self::TranslateGlossary => Some(Stage::Translation),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QcBefore {
    pub source: Option<SourceText>,
    pub translation: Option<Translation>,
    pub typography: Option<Typography>,
    pub geometry: Option<Geometry>,
    pub visibility: Option<Visibility>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QcAutoFix {
    pub reason: QcCheck,
    pub action: QcFixAction,
    pub before: QcBefore,
}

pub(crate) fn auto_fixes(snapshot: &Snapshot, layer: EntityId) -> Result<Vec<QcAutoFix>> {
    snapshot
        .component::<Typography>(layer)?
        .and_then(|typography| typography.extensions.get(HISTORY).cloned())
        .map(|json| serde_json::from_str(&json).context("invalid QC fix history"))
        .transpose()
        .map(Option::unwrap_or_default)
}

fn before(snapshot: &Snapshot, layer: EntityId) -> Result<QcBefore> {
    let content = snapshot.text_layer(layer)?.content()?;
    let mut typography = snapshot.component::<Typography>(layer)?;
    if let Some(typography) = &mut typography {
        // Prior records live alongside this one, rather than recursively inside it.
        typography.extensions.remove(HISTORY);
    }
    Ok(QcBefore {
        source: content.source()?,
        translation: content.translation()?,
        typography,
        geometry: snapshot.component::<Geometry>(layer)?,
        visibility: snapshot.component::<Visibility>(layer)?,
    })
}

fn hand_edited(snapshot: &Snapshot, context: &ProjectContext, layer: EntityId) -> Result<bool> {
    let content = snapshot.text_layer(layer)?.content()?;
    Ok(snapshot
        .component::<EntityOrigin>(layer)?
        .is_some_and(|v| v.origin == Origin::User)
        || content.source()?.is_some_and(|v| v.text.origin == Origin::User)
        || content
            .translation()?
            .is_some_and(|v| v.text.origin == Origin::User)
        || snapshot
            .component::<Typography>(layer)?
            .is_none_or(|v| v.origin == Origin::User)
        || snapshot
            .component::<Geometry>(layer)?
            .is_some_and(|v| v.origin == Origin::User)
        || snapshot
            .component::<TextLayout>(layer)?
            .is_some_and(|v| v.origin == Origin::User)
        || snapshot
            .component::<Visibility>(layer)?
            .is_some_and(|v| v.origin == Origin::User)
        || context
            .category(layer)
            .is_some_and(|v| v.source == LabelSource::User))
}

fn attempts(context: &ProjectContext, layer: EntityId) -> u8 {
    context
        .lines
        .get(&layer)
        .and_then(|line| line.other.get(ATTEMPTS))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
        .min(2) as u8
}

fn findings_for_layer(
    findings: &[QcFinding],
    snapshot: &Snapshot,
    context: &ProjectContext,
    layer: EntityId,
) -> Result<Vec<QcFinding>> {
    let hints = super::super::context::translation_hints(context, snapshot)?;
    let model = koharu_pipeline::PipelineConfig::load()?
        .read()?
        .translation
        .model
        .clone();
    let supports_instructions = koharu_translator::Translator::supports_tasks(&model);
    Ok(findings
        .iter()
        .filter(|finding| finding.layer == layer)
        .filter(|finding| {
            if hints.skip.contains(&layer)
                && matches!(
                    finding.check,
                    QcCheck::MissingTranslation | QcCheck::LeftoverSource | QcCheck::Flag
                )
            {
                return false;
            }
            if !supports_instructions
                && matches!(finding.check, QcCheck::LeftoverSource | QcCheck::Flag)
            {
                return false;
            }
            if let Some(booharu_context::Flag::Glossary { term, .. }) = &finding.flag {
                return hints.glossary.iter().any(|entry| {
                    entry.locked && entry.hint.source.trim() == term.trim()
                });
            }
            true
        })
        .cloned()
        .collect())
}

fn isolated_model_target(snapshot: &Snapshot, layer: EntityId) -> Result<bool> {
    use koharu_scene::{FitsTo, Presents, RecognizedFrom, RelationSpec};
    let mut selected = BTreeSet::from([layer]);
    let mut pending = vec![layer];
    while let Some(entity) = pending.pop() {
        for relation in snapshot
            .relations_from(entity, None)
            .chain(snapshot.relations_to(entity, None))
        {
            let relation = relation.value();
            if ![Presents::KIND, RecognizedFrom::KIND, FitsTo::KIND]
                .contains(&relation.kind.as_str())
            {
                continue;
            }
            for related in [relation.source, relation.target] {
                if selected.insert(related) {
                    pending.push(related);
                }
            }
        }
    }
    // The ordinary pipeline's entity scope follows semantic relations. A shared
    // source/content would change another box, so leave it for explicit review.
    Ok(!snapshot
        .text_layers()?
        .any(|text| text.id() != layer && selected.contains(&text.id())))
}

fn can_run_action(snapshot: &Snapshot, layer: EntityId, action: QcFixAction) -> Result<bool> {
    if action.stage().is_some() && !isolated_model_target(snapshot, layer)? {
        return Ok(false);
    }
    if action == QcFixAction::ShortenTranslation {
        let model = koharu_pipeline::PipelineConfig::load()?
            .read()?
            .translation
            .model
            .clone();
        return Ok(koharu_translator::Translator::supports_tasks(&model));
    }
    Ok(true)
}

fn targets(snapshot: &Snapshot, scope: &QcFixScope) -> Result<Vec<(EntityId, EntityId)>> {
    match scope {
        QcFixScope::Pages { pages } => {
            for &page in pages {
                snapshot.page(page)?;
            }
        }
        QcFixScope::Boxes { layers } => {
            for &layer in layers {
                snapshot.text_layer(layer)?;
            }
        }
        QcFixScope::Project => {}
    }
    let mut result = Vec::new();
    for page in snapshot.pages() {
        for layer in page.text_layers()? {
            if match scope {
                QcFixScope::Project => true,
                QcFixScope::Pages { pages } => pages.contains(&page.id()),
                QcFixScope::Boxes { layers } => layers.contains(&layer.id()),
            } {
                result.push((page.id(), layer.id()));
            }
        }
    }
    Ok(result)
}

pub(crate) fn summary(project: &Project) -> Result<QcFixSummary> {
    project
        .load_context()?
        .other
        .get(SUMMARY)
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map(|v| v.unwrap_or_default())
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_qc_fix_summary(
    project: State<'_, CurrentProject>,
) -> Result<QcFixSummary, Error> {
    let projects = project.project.lock().await;
    Ok(summary(projects.as_ref().context("no project is open")?)?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn fix_qc(
    handle: AppHandle<CefRuntime>,
    scope: QcFixScope,
    automatic: Option<bool>,
) -> Result<Option<JobId>, Error> {
    Ok(start_fix_job(handle, scope, automatic.unwrap_or(false)).await?)
}

pub(crate) async fn start_fix_job(
    handle: AppHandle<CefRuntime>,
    scope: QcFixScope,
    automatic: bool,
) -> Result<Option<JobId>> {
    if automatic && !koharu_config::load::<super::QcSettings>("qc")?.read()?.auto_fix {
        return Ok(None);
    }
    ensure!(
        !handle.state::<super::super::runs::BatchState>().active(),
        "a chapter batch is running"
    );
    ensure!(
        handle.state::<Processing>().stops.lock().is_empty(),
        "another process is already running"
    );
    // Refresh findings before reserving a job; exhausted or manual boxes must not
    // generate an endless chain of empty automatic jobs.
    let report = super::run_checks(
        QcScope::Project,
        None,
        automatic,
        &handle.state::<CurrentProject>(),
        &handle.state::<Desktop>(),
        None,
    )
    .await
    .map_err(|error| error.0)?;
    let scope = if automatic {
        let pages = report
            .findings
            .iter()
            .map(|finding| finding.page)
            .collect::<BTreeSet<_>>();
        match scope {
            QcFixScope::Project => QcFixScope::Pages {
                pages: pages.into_iter().collect(),
            },
            QcFixScope::Pages { pages: selected } => QcFixScope::Pages {
                pages: selected.into_iter().filter(|page| pages.contains(page)).collect(),
            },
            QcFixScope::Boxes { layers } => QcFixScope::Boxes {
                layers: layers
                    .into_iter()
                    .filter(|layer| report.findings.iter().any(|finding| finding.layer == *layer))
                    .collect(),
            },
        }
    } else {
        scope
    };
    let (name, expected) = {
        let projects = handle.state::<CurrentProject>();
        let projects = projects.project.lock().await;
        let project = projects.as_ref().context("no project is open")?;
        let snapshot = project.snapshot();
        let context = project.load_context()?;
        let mut eligible = false;
        for (_, layer) in targets(&snapshot, &scope)? {
            let findings = findings_for_layer(&report.findings, &snapshot, &context, layer)?;
            let history = auto_fixes(&snapshot, layer)?
                .iter()
                .map(|v| v.action)
                .collect::<Vec<_>>();
            if let Some((_, action)) = next_fix(
                &findings,
                hand_edited(&snapshot, &context, layer)?,
                attempts(&context, layer),
                &history,
            ) {
                eligible |= can_run_action(&snapshot, layer, action)?;
            }
        }
        if !eligible {
            let selected = targets(&snapshot, &scope)?
                .into_iter()
                .map(|(_, layer)| layer)
                .collect::<BTreeSet<_>>();
            let items_left = super::review::list(project)?
                .into_iter()
                .filter(|item| selected.contains(&item.finding.layer))
                .map(|item| item.finding.layer)
                .collect::<BTreeSet<_>>()
                .len() as u32;
            if !automatic {
                let value = serde_json::to_value(QcFixSummary {
                    items_left,
                    ..Default::default()
                })?;
                project.update_context(|context| {
                    context.other.insert(SUMMARY.into(), value);
                })?;
            }
            return Ok(None);
        }
        (project.name.clone(), snapshot.project_id())
    };
    let finish_sound = super::super::runs::RunSettings::load()?.finish_sound;
    let id = JobId::new();
    let stop = StopToken::default();
    {
        let processing = handle.state::<Processing>();
        let mut stops = processing.stops.lock();
        ensure!(stops.is_empty(), "another process is already running");
        ensure!(
            !handle.state::<super::super::runs::BatchState>().active(),
            "a chapter batch is running"
        );
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
        project: Some(name),
        operation: None,
        finish_sound,
        auto_run: None,
    };
    handle.state::<Processing>().jobs.lock().insert(id, job.clone());
    handle.state::<JobChannel>().channel.publish(job);
    drop(tokio::spawn(async move {
        let result = run_fix_pass(&handle, scope, &stop, id, expected).await;
        handle.state::<Processing>().stops.lock().remove(&id);
        update_job(&handle, id, |job| {
            job.state = if stop.stopped() {
                JobState::Stopped
            } else if result.is_err() {
                JobState::Failed
            } else if job.failures.is_empty() {
                JobState::Finished
            } else {
                JobState::FinishedWithProblems
            };
            job.error = result.err().map(|error| format!("{error:#}"));
        });
    }));
    Ok(Some(id))
}

fn update_job(handle: &AppHandle<CefRuntime>, id: JobId, change: impl FnOnce(&mut Job)) {
    let job = {
        let processing = handle.state::<Processing>();
        let mut jobs = processing.jobs.lock();
        jobs.get_mut(&id).map(|job| {
            change(job);
            job.clone()
        })
    };
    if let Some(job) = job {
        handle.state::<JobChannel>().channel.publish(job);
    }
}

/// Auto-run integration: use its existing job and StopToken, then include the
/// returned counters in its summary. Bind to the run's project, even if the user
/// switches projects before this phase starts. The durable budget includes failures.
pub(crate) async fn run_fix_pass(
    handle: &AppHandle<CefRuntime>,
    scope: QcFixScope,
    stop: &StopToken,
    job: JobId,
    expected: ProjectId,
) -> Result<QcFixSummary> {
    let selected = {
        let projects = handle.state::<CurrentProject>();
        let projects = projects.project.lock().await;
        let project = projects.as_ref().context("no project is open")?;
        let snapshot = project.snapshot();
        ensure!(
            snapshot.project_id() == expected,
            "the project changed before QC repair started"
        );
        targets(&snapshot, &scope)?
    };
    let mut summary = QcFixSummary::default();
    update_job(handle, job, |job| {
        job.total = selected.len();
        job.completed = 0;
    });
    for (index, &(page, layer)) in selected.iter().enumerate() {
        if stop.stopped() {
            break;
        }
        update_job(handle, job, |job| {
            job.page = Some(page);
            job.stage = None;
            job.model = None;
        });
        for _ in 0..2 {
            if stop.stopped() {
                break;
            }
            let report = super::run_checks(
                QcScope::Page { page },
                None,
                false,
                &handle.state::<CurrentProject>(),
                &handle.state::<Desktop>(),
                None,
            )
            .await
            .map_err(|error| error.0)?;
            let planned = {
                let projects = handle.state::<CurrentProject>();
                let projects = projects.project.lock().await;
                let project = projects.as_ref().context("no project is open")?;
                let snapshot = project.snapshot();
                ensure!(
                    snapshot.project_id() == expected,
                    "the project changed during QC repair"
                );
                let context = project.load_context()?;
                let findings = findings_for_layer(&report.findings, &snapshot, &context, layer)?;
                let history = auto_fixes(&snapshot, layer)?
                    .iter()
                    .map(|v| v.action)
                    .collect::<Vec<_>>();
                next_fix(
                    &findings,
                    hand_edited(&snapshot, &context, layer)?,
                    attempts(&context, layer),
                    &history,
                )
                .map(|(reason, action)| (snapshot, context, reason, action))
            };
            let Some((snapshot, context, reason, action)) = planned else {
                break;
            };
            if !can_run_action(&snapshot, layer, action)? {
                break;
            }
            // Count before a request starts, so failure or reopening cannot reset the cap.
            {
                let projects = handle.state::<CurrentProject>();
                let projects = projects.project.lock().await;
                let project = projects.as_ref().context("no project is open")?;
                ensure!(
                    project.snapshot().project_id() == expected,
                    "the project changed during QC repair"
                );
                project.update_context(|context| {
                    let next = attempts(context, layer) + 1;
                    context
                        .lines
                        .entry(layer)
                        .or_default()
                        .other
                        .insert(ATTEMPTS.into(), next.into());
                })?;
            }
            let result = if let Some(stage) = action.stage() {
                summary.requests_used += 1;
                model_fix(
                    handle, &snapshot, &context, layer, reason, action, stage, stop, job,
                )
                .await
            } else {
                mechanical_fix(
                    handle, &snapshot, &context, page, layer, reason, action, stop,
                )
                .await
            };
            match result {
                Ok(true) => summary.fixes_made += 1,
                Ok(false) => {}
                Err(error) => {
                    {
                        let projects = handle.state::<CurrentProject>();
                        let projects = projects.project.lock().await;
                        if let Some(project) = projects
                            .as_ref()
                            .filter(|project| project.snapshot().project_id() == expected)
                        {
                            project.update_context(|context| {
                                context
                                    .lines
                                    .entry(layer)
                                    .or_default()
                                    .other
                                    .insert(ATTEMPTS.into(), 2.into());
                            })?;
                        }
                    }
                    update_job(handle, job, |job| {
                        job.failures.push(koharu_pipeline::PageFailure {
                            page,
                            label: snapshot
                                .page(page)
                                .and_then(|p| p.page())
                                .map(|p| p.label)
                                .unwrap_or_default(),
                            stage: action.stage().unwrap_or(Stage::Translation),
                            error: format!("{error:#}"),
                        });
                    });
                    // A failed model request is already bounded by its provider and
                    // must not acquire a second QC retry on the same run.
                    break;
                }
            }
        }
        update_job(handle, job, |job| job.completed = index + 1);
    }
    super::run_checks(
        QcScope::Project,
        None,
        false,
        &handle.state::<CurrentProject>(),
        &handle.state::<Desktop>(),
        None,
    )
    .await
    .map_err(|error| error.0)?;
    let selected_layers = selected.iter().map(|(_, layer)| *layer).collect::<BTreeSet<_>>();
    let projects = handle.state::<CurrentProject>();
    let projects = projects.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    ensure!(
        project.snapshot().project_id() == expected,
        "the project changed during QC repair"
    );
    summary.items_left = super::review::list(project)?
        .into_iter()
        .filter(|item| selected_layers.contains(&item.finding.layer))
        .map(|item| item.finding.layer)
        .collect::<BTreeSet<_>>()
        .len() as u32;
    let value = serde_json::to_value(summary)?;
    project.update_context(|context| {
        context.other.insert(SUMMARY.into(), value);
    })?;
    Ok(summary)
}

async fn synchronize(
    handle: &AppHandle<CefRuntime>,
    commit: &koharu_scene::Commit,
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

fn clear_acknowledgement(project: &Project, layer: EntityId) -> Result<()> {
    project.update_context(|context| {
        context
            .lines
            .entry(layer)
            .or_default()
            .qc_ok
            .retain(|id| id != "auto_fixed");
    })
}

async fn mechanical_fix(
    handle: &AppHandle<CefRuntime>,
    snapshot: &Snapshot,
    context: &ProjectContext,
    page: EntityId,
    layer: EntityId,
    reason: QcCheck,
    action: QcFixAction,
    stop: &StopToken,
) -> Result<bool> {
    let original = before(snapshot, layer)?;
    let mut typography = snapshot
        .component::<Typography>(layer)?
        .context("text layer has no typography")?;
    let generation = match &typography.origin {
        Origin::Generated(value) => value.clone(),
        Origin::User => return Ok(false),
    };
    let mut geometry = None;
    let mut visibility = None;
    match action {
        QcFixAction::TightenLayout => {
            typography.line_spacing = Some(typography.line_spacing.unwrap_or(1.2).min(1.0));
            typography
                .extensions
                .insert("dev.booharu.qc.hyphenation".into(), "normal".into());
            // Preview tightening first; widening is needed only if it still fails.
            let mut edit = snapshot.edit_as(generation.clone());
            edit.set(layer, &typography)?;
            let preview = snapshot.preview([&edit.finish()?])?;
            let renderer = handle.state::<Desktop>().renderer();
            let frame = renderer.render(&preview, page).await?;
            let minimum = koharu_config::load::<super::QcSettings>("qc")?
                .read()?
                .min_font_size;
            let needs_width = frame.diagnostics().iter().any(|diagnostic| {
                matches!(diagnostic,
                    koharu_renderer::RenderDiagnostic::TextOverflow { entity, .. } if *entity == layer)
            }) || frame.layer(layer).is_some_and(|rendered| {
                matches!(rendered.kind(),
                    koharu_renderer::LayerKind::Text(text) if text.smallest_font_size < minimum)
            });
            if needs_width {
                geometry = widened_geometry(snapshot, layer)?;
            }
        }
        QcFixAction::ApplyCategoryStyle => {
            let styles = super::super::styles::load_text_styles()?.read()?.clone();
            let settings = super::super::lettering::LetteringSettings::load()?.read()?.clone();
            let info = koharu_pipeline::bubble_shape::text_bubble_shape(snapshot, layer)?;
            let category = super::super::lettering::resolve_category(
                context, snapshot, layer, &info, &settings,
            )?
            .category;
            let colors = koharu_pipeline::PipelineConfig::load()?
                .read()?
                .detect_text_color;
            styles
                .default_style
                .apply_to_detected(&mut typography, colors);
            styles
                .category_style(category.as_str())
                .apply_to_detected(&mut typography, colors);
            if original.typography.as_ref() == Some(&typography) {
                return Ok(false);
            }
        }
        QcFixAction::HideEmpty => {
            visibility = Some(Visibility {
                origin: Origin::Generated(generation.clone()),
                visible: false,
                opacity: original.visibility.as_ref().map_or(1.0, |value| value.opacity),
            });
        }
        _ => unreachable!("model repairs use the pipeline"),
    }
    let mut history = auto_fixes(snapshot, layer)?;
    history.push(QcAutoFix {
        reason,
        action,
        before: original,
    });
    typography
        .extensions
        .insert(HISTORY.into(), serde_json::to_string(&history)?);
    let mut edit = snapshot.edit_as(generation);
    // Observe the whole page: both the rendered result and bubble containment
    // depend on neighbours, geometry and source text, not just typography.
    edit.observe_subtree(page)?;
    edit.set(layer, &typography)?;
    if let Some(geometry) = geometry {
        edit.set(layer, &geometry)?;
    }
    if let Some(visibility) = visibility {
        edit.set(layer, &visibility)?;
    }
    let patch = edit.finish()?;
    let (commit, active) = {
        let projects = handle.state::<CurrentProject>();
        let mut projects = projects.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let current_context = project.load_context()?;
        if stop.stopped() || hand_edited(&project.snapshot(), &current_context, layer)? {
            return Ok(false);
        }
        ensure!(
            current_context.category(layer) == context.category(layer),
            "lettering category changed during QC repair"
        );
        ensure!(
            project.snapshot().project_id() == snapshot.project_id(),
            "the project changed during QC repair"
        );
        let Some(commit) = project.commit_rebased(patch).await? else {
            return Ok(false);
        };
        project.record_commit(&commit);
        clear_acknowledgement(project, layer)?;
        (commit, project.active_page())
    };
    synchronize(handle, &commit, active).await?;
    Ok(true)
}

struct CollectedOutput {
    snapshot: Snapshot,
    patch: Option<Patch>,
}

#[async_trait::async_trait]
impl Committer for CollectedOutput {
    async fn commit(&mut self, output: StageOutput) -> Result<Snapshot> {
        ensure!(
            self.patch.is_none(),
            "a QC request must contain exactly one stage and one box"
        );
        let preview = self.snapshot.preview([&output.patch])?;
        self.patch = Some(output.patch);
        Ok(preview)
    }
}

async fn model_fix(
    handle: &AppHandle<CefRuntime>,
    snapshot: &Snapshot,
    context: &ProjectContext,
    layer: EntityId,
    reason: QcCheck,
    action: QcFixAction,
    stage: Stage,
    stop: &StopToken,
    job: JobId,
) -> Result<bool> {
    let original = before(snapshot, layer)?;
    let series = serde_json::to_value(super::super::context::linked_series(context)?)?;
    let mut hints = super::super::context::translation_hints(context, snapshot)?;
    hints.skip.extend(
        snapshot
            .text_layers()?
            .map(|text| text.id())
            .filter(|id| *id != layer),
    );
    hints.instructions = match action {
        QcFixAction::ShortenTranslation => Some(format!(
            "Use a shorter natural wording, preserving meaning and every required glossary term. Aim for at most {} characters including spaces. Do not omit names or facts.",
            original.translation.as_ref().map_or(1, |value| {
                (value.text.value.chars().count() * 3 / 4).max(1)
            }),
        )),
        QcFixAction::TranslateLeftover => Some("Translate all Japanese into the target language; romanize names only when no glossary spelling exists. Use the supplied glossary spellings for names. Leave no Japanese script in the translation.".into()),
        QcFixAction::TranslateGlossary => Some("The previous translation missed a locked glossary term. Include every applicable glossary target exactly as supplied, including names, while translating the complete line.".into()),
        _ => None,
    };
    let progress_handle = handle.clone();
    let request = koharu_pipeline::Request {
        operation: Operation::Only { stage },
        scope: Scope::Entities(vec![layer]),
        stop: stop.clone(),
        translation_hints: Arc::new(hints),
        progress: Some(Arc::new(move |event| {
            if let koharu_pipeline::Progress::Loading { page, stage, model }
                | koharu_pipeline::Progress::Running { page, stage, model } = event
            {
                update_job(&progress_handle, job, |job| {
                    job.page = Some(page);
                    job.stage = Some(stage);
                    job.model = Some(model);
                });
            }
        })),
        // QC owns the two-attempt cap; provider-native bounded structured-output
        // retries and accelerator OOM recovery remain owned by the pipeline.
        policy: koharu_pipeline::RunPolicy {
            retries: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut output = CollectedOutput {
        snapshot: snapshot.clone(),
        patch: None,
    };
    let report = handle
        .state::<koharu_pipeline::Pipeline>()
        .execute(snapshot.clone(), request, &mut output)
        .await?;
    if let Some(failure) = report.failures.first() {
        anyhow::bail!("{}", failure.error);
    }
    if stop.stopped() {
        return Ok(false);
    }
    if stage == Stage::Translation {
        let Some(patch) = &output.patch else {
            return Ok(false);
        };
        let preview = snapshot.preview([patch])?;
        let next = preview
            .text_layer(layer)?
            .content()?
            .translation()?
            .map(|translation| translation.text.value)
            .unwrap_or_default();
        let previous = original
            .translation
            .as_ref()
            .map_or("", |translation| translation.text.value.as_str());
        if next.trim().is_empty()
            || next == previous
            || (action == QcFixAction::ShortenTranslation
                && next.chars().count() >= previous.chars().count())
        {
            return Ok(false);
        }
    }
    let patch = output.patch;
    let record = QcAutoFix {
        reason,
        action,
        before: original.clone(),
    };
    let mut history = auto_fixes(snapshot, layer)?;
    history.push(record);
    let history = serde_json::to_string(&history)?;
    ensure!(
        history.len() <= 64 * 1024,
        "QC history exceeds the typography extension limit"
    );
    let (commit, active) = {
        let projects = handle.state::<CurrentProject>();
        let mut projects = projects.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        ensure!(
            project.snapshot().project_id() == snapshot.project_id(),
            "the project changed during QC repair"
        );
        if stop.stopped() || hand_edited(&project.snapshot(), &project.load_context()?, layer)? {
            return Ok(false);
        }
        // Context is part of the translation input, and the page is part of the
        // vision input. Do not accept derived wording after either changes.
        let mut observation = snapshot.edit();
        for page in snapshot.pages() {
            observation.observe_subtree(page.id())?;
        }
        observation.finish()?.validate_on(&project.snapshot())?;
        let mut current_context = project.load_context()?;
        let mut observed_context = context.clone();
        for value in [&mut current_context, &mut observed_context] {
            for line in value.lines.values_mut() {
                line.other.remove(ATTEMPTS);
            }
            value
                .lines
                .retain(|_, line| *line != booharu_context::LineContext::default());
        }
        ensure!(
            current_context == observed_context,
            "context changed during QC repair"
        );
        ensure!(
            serde_json::to_value(super::super::context::linked_series(&current_context)?)? == series,
            "glossary changed during QC repair"
        );
        let mut commit = match patch {
            Some(patch) => project.commit_rebased(patch).await?,
            None => None,
        };
        let mut revisions = commit.iter().map(|commit| commit.revision).collect::<Vec<_>>();
        let completion: Result<()> = async {
            if stage == Stage::Translation
                && let Some(translated) = &commit
            {
                if let Some(reset) =
                    super::super::rich_text::after_translation(project, translated, true).await?
                {
                    revisions.push(reset.revision);
                    commit = Some(reset);
                }
                super::super::context::refresh_flags(project)?;
            }
            let current = project.snapshot();
            let mut typography = current
                .component::<Typography>(layer)?
                .context("text layer has no typography")?;
            typography.extensions.insert(HISTORY.into(), history);
            // Translation resets style; retain the first layout attempt and its
            // history so shortening cannot undo an already useful fit adjustment.
            if let Some(previous) = snapshot.component::<Typography>(layer)? {
                if let Some(value) = previous.extensions.get("dev.booharu.qc.hyphenation") {
                    typography
                        .extensions
                        .insert("dev.booharu.qc.hyphenation".into(), value.clone());
                    typography.line_spacing = previous.line_spacing;
                }
            }
            let marker =
                current.patch(|edit| edit.set_typography_preserving_origin(layer, &typography))?;
            let marked = project.commit(marker).await?;
            revisions.push(marked.revision);
            commit = Some(marked);
            clear_acknowledgement(project, layer)?;
            Ok(())
        }
        .await;
        // Even a sidecar/disk failure must leave completed changes undoable.
        if !revisions.is_empty() {
            project.record(revisions);
        }
        completion?;
        (
            commit.context("QC history was not committed")?,
            project.active_page(),
        )
    };
    synchronize(handle, &commit, active).await?;
    Ok(true)
}

fn widened_geometry(snapshot: &Snapshot, layer: EntityId) -> Result<Option<Geometry>> {
    let layout = snapshot.text_layer(layer)?.layout()?;
    if layout.angle_degrees.is_some_and(|angle| angle != 0.0) || layout.warp.is_some() {
        return Ok(None);
    }
    // Automatic balloon placement already uses the bubble contour. Only a
    // smaller authored presentation frame can gain width without leaving it.
    let Some(current) = snapshot.component::<Geometry>(layer)? else {
        return Ok(None);
    };
    let typography = snapshot
        .component::<Typography>(layer)?
        .context("text layer has no typography")?;
    if !matches!((&current.origin, &typography.origin),
        (Origin::Generated(a), Origin::Generated(b)) if a.producer == b.producer)
    {
        return Ok(None);
    }
    let info = koharu_pipeline::bubble_shape::text_bubble_shape(snapshot, layer)?;
    let Some(bubble) = info.bubble else {
        return Ok(None);
    };
    let Some(boundary) = snapshot.component::<Geometry>(bubble)? else {
        return Ok(None);
    };
    Ok(widen_inside(&current, &boundary))
}

fn widen_inside(current: &Geometry, bubble: &Geometry) -> Option<Geometry> {
    if current.points.len() != 4 || bubble.points.len() < 3 {
        return None;
    }
    let left = current.points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let right = current
        .points
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let top = current.points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let bottom = current
        .points
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    // Preserve transformed/non-rectangular frames. Detection polygons remain immutable.
    if current.points.iter().any(|point| {
        (point.x != left && point.x != right) || (point.y != top && point.y != bottom)
    }) {
        return None;
    }
    let width = right - left;
    for fraction in [0.15, 0.10, 0.05] {
        let mut candidate = Geometry::rectangle(
            left - width * fraction,
            top,
            width * (1.0 + 2.0 * fraction),
            bottom - top,
        );
        candidate.origin = current.origin.clone();
        if polygon_contains_rectangle(&bubble.points, &candidate.points) {
            return Some(candidate);
        }
    }
    None
}

fn polygon_contains_rectangle(polygon: &[koharu_scene::Point], rect: &[koharu_scene::Point]) -> bool {
    fn inside(p: koharu_scene::Point, polygon: &[koharu_scene::Point]) -> bool {
        let mut inside = false;
        for (a, b) in polygon
            .iter()
            .zip(polygon.iter().cycle().skip(1))
            .take(polygon.len())
        {
            if (a.y > p.y) != (b.y > p.y)
                && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
            {
                inside = !inside;
            }
        }
        inside
    }
    fn cross(a: koharu_scene::Point, b: koharu_scene::Point, c: koharu_scene::Point) -> f64 {
        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
    }
    rect.iter().all(|point| inside(*point, polygon))
        && !rect
            .iter()
            .zip(rect.iter().cycle().skip(1))
            .take(4)
            .any(|(a, b)| {
                polygon
                    .iter()
                    .zip(polygon.iter().cycle().skip(1))
                    .take(polygon.len())
                    .any(|(c, d)| {
                        cross(*a, *b, *c) * cross(*a, *b, *d) <= 0.0
                            && cross(*c, *d, *a) * cross(*c, *d, *b) <= 0.0
                    })
            })
}
