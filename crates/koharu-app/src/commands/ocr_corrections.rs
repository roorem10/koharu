use std::collections::BTreeSet;

use anyhow::{Context as _, Result, ensure};
use booharu_context::{Flag, OcrCorrection, ProjectContext};
use koharu_desktop::Desktop;
use koharu_pipeline::Scope;
use koharu_scene::{
    AssetRole, Authored, Commit, EntityId, Generation, Origin, ProducerId, Snapshot, SourceText,
};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager as _, State};
use tauri_runtime_cef::CefRuntime;

use super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    project::{CurrentProject, Project},
};

const OCR_PRODUCER: &str = "dev.koharu.pipeline.ocr";
const CORRECTION_MODEL: &str = "booharu-ocr-correction";
const TRANSLATION_BEFORE: &str = "ocr_correction_translation";
const STALE_DISMISSED: &str = "ocr_source_changed";
const EVIDENCE: &str = "ocr_correction_evidence";

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OcrCorrectionStatus {
    Pending,
    Applied,
    Dismissed,
    Stale,
    HandEdited,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OcrCheckSkipReason {
    NoVisionModel,
    Disabled,
}

#[derive(Serialize, Type)]
pub(crate) struct OcrSuggestion {
    pub layer: EntityId,
    pub page: EntityId,
    pub original: String,
    pub suggested: String,
    pub reason: String,
    pub status: OcrCorrectionStatus,
}

#[derive(Serialize, Type)]
pub(crate) struct OcrSuggestions {
    pub suggestions: Vec<OcrSuggestion>,
    pub skip_reason: Option<OcrCheckSkipReason>,
}

#[derive(Default, Serialize, Type)]
pub(crate) struct OcrCorrectionResult {
    pub applied: u32,
    pub pages: u32,
    pub skip_reason: Option<OcrCheckSkipReason>,
}

// Source ownership is authoritative even when a hand edit happens to equal the OCR.
pub(super) fn is_ocr_source(source: &SourceText) -> bool {
    !source.text.value.trim().is_empty()
        && matches!(&source.text.origin, Origin::Generated(generation)
            if generation.producer.as_str() == OCR_PRODUCER
                && generation.model.as_deref() != Some(CORRECTION_MODEL))
}

// Suggestions remain valid across unrelated edits, but never across a changed
// image, recognition region or source ownership, even if the OCR string matches.
pub(super) fn evidence(
    snapshot: &Snapshot,
    layer: EntityId,
    page: EntityId,
) -> Result<serde_json::Value> {
    let layer = snapshot.text_layer(layer)?;
    let content = layer.content()?;
    let region = content.source_region()?;
    let geometry = match region {
        Some(region) => Some(region.geometry()?),
        None => layer.frame()?,
    };
    Ok(serde_json::json!({
        "source": content.source()?,
        "image": snapshot.asset(page, &AssetRole::new("source")?)?.map(|asset| asset.blob),
        "region": region.map(|region| region.id()),
        "geometry": geometry,
    }))
}

fn correction_status(source: Option<&SourceText>, correction: &OcrCorrection) -> OcrCorrectionStatus {
    let Some(source) = source else {
        return OcrCorrectionStatus::Stale;
    };
    let Origin::Generated(generation) = &source.text.origin else {
        return OcrCorrectionStatus::HandEdited;
    };
    if generation.producer.as_str() == OCR_PRODUCER
        && generation.model.as_deref() == Some(CORRECTION_MODEL)
        && source.text.value == correction.suggested
    {
        return OcrCorrectionStatus::Applied;
    }
    if correction.dismissed {
        return OcrCorrectionStatus::Dismissed;
    }
    if is_ocr_source(source)
        && source.text.value == correction.original
        && !correction.suggested.trim().is_empty()
        && !correction.reason.trim().is_empty()
        && correction.original != correction.suggested
    {
        OcrCorrectionStatus::Pending
    } else {
        OcrCorrectionStatus::Stale
    }
}

pub(crate) fn suggestions(project: &Project, scope: &Scope) -> Result<OcrSuggestions> {
    let snapshot = project.snapshot();
    let context = project.load_context()?;
    match scope {
        Scope::Pages(ids) => {
            for id in ids {
                snapshot.page(*id)?;
            }
        }
        Scope::Entities(ids) => {
            for id in ids {
                snapshot.text_layer(*id)?;
            }
        }
        Scope::Region { page, bounds } => {
            snapshot.page(*page)?;
            ensure!(
                [bounds.x, bounds.y, bounds.width, bounds.height]
                    .into_iter()
                    .all(f64::is_finite)
                    && bounds.width > 0.0
                    && bounds.height > 0.0,
                "region bounds must be finite and non-empty"
            );
        }
        Scope::Project => {}
    }
    let mut suggestions = Vec::new();
    for page in snapshot.pages() {
        if matches!(scope, Scope::Pages(ids) if !ids.contains(&page.id()))
            || matches!(scope, Scope::Region { page: id, .. } if *id != page.id())
        {
            continue;
        }
        if let Some(group) = page.text_group()? {
            for layer in group.text_layers()? {
                if matches!(scope, Scope::Entities(ids) if !ids.contains(&layer.id())) {
                    continue;
                }
                if let Scope::Region { bounds, .. } = scope {
                    let Some(frame) = layer.frame()? else {
                        continue;
                    };
                    let min_x = frame.points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
                    let min_y = frame.points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
                    let max_x = frame
                        .points
                        .iter()
                        .map(|p| p.x)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let max_y = frame
                        .points
                        .iter()
                        .map(|p| p.y)
                        .fold(f64::NEG_INFINITY, f64::max);
                    if min_x >= bounds.x + bounds.width
                        || max_x <= bounds.x
                        || min_y >= bounds.y + bounds.height
                        || max_y <= bounds.y
                    {
                        continue;
                    }
                }
                let Some(line) = context.lines.get(&layer.id()) else {
                    continue;
                };
                let Some(correction) = line.ocr_correction.as_ref() else {
                    continue;
                };
                let mut status = correction_status(layer.content()?.source()?.as_ref(), correction);
                if status == OcrCorrectionStatus::Pending
                    && line.other.get(EVIDENCE) != Some(&evidence(&snapshot, layer.id(), page.id())?)
                {
                    status = OcrCorrectionStatus::Stale;
                }
                suggestions.push(OcrSuggestion {
                    layer: layer.id(),
                    page: page.id(),
                    original: correction.original.clone(),
                    suggested: correction.suggested.clone(),
                    reason: correction.reason.clone(),
                    status,
                });
            }
        }
    }
    Ok(OcrSuggestions {
        suggestions,
        skip_reason: context
            .other
            .get("ocr_check_skip_reason")
            .and_then(|value| value.as_str())
            .filter(|reason| *reason == "no_vision_model")
            .map(|_| OcrCheckSkipReason::NoVisionModel),
    })
}

// Flags derive from the current Scene, so undo/redo also restore their meaning.
pub(crate) fn refresh_translation_flags(
    snapshot: &Snapshot,
    context: &mut ProjectContext,
) -> Result<()> {
    for (number, page) in snapshot.pages().enumerate() {
        if let Some(group) = page.text_group()? {
            for layer in group.text_layers()? {
                let Some(line) = context.lines.get_mut(&layer.id()) else {
                    continue;
                };
                let content = layer.content()?;
                let source = content.source()?;
                let applied = line.ocr_correction.as_ref().is_some_and(|correction| {
                    correction_status(source.as_ref(), correction) == OcrCorrectionStatus::Applied
                });
                let translation = content.translation()?;
                let translation_value = translation.as_ref().map(serde_json::to_value).transpose()?;
                // Translation has no run identity. An identical rerun remains a review
                // warning until dismissed; unrelated stage completions cannot clear it.
                let stale = applied
                    && translation
                        .as_ref()
                        .is_some_and(|translation| !translation.text.value.trim().is_empty())
                    && line.other.get(TRANSLATION_BEFORE) == translation_value.as_ref();
                line.flags
                    .retain(|flag| !matches!(flag, Flag::OcrSourceChanged { .. }));
                if stale {
                    if !line.dismissed.iter().any(|value| value == STALE_DISMISSED) {
                        line.flags.push(Flag::OcrSourceChanged {
                            page: number as u32 + 1,
                        });
                    }
                } else {
                    line.dismissed.retain(|value| value != STALE_DISMISSED);
                }
            }
        }
    }
    Ok(())
}

async fn apply_page(
    project: &mut Project,
    page: EntityId,
    layers: &[EntityId],
) -> Result<Option<(Commit, u32)>> {
    let snapshot = project.snapshot();
    let context = project.load_context()?;
    // A vision correction is a second OCR pass. Keep the OCR producer because Scene
    // forbids generated work from taking over another producer's source component.
    let mut generation = Generation::new(ProducerId::new(OCR_PRODUCER)?);
    generation.model = Some(CORRECTION_MODEL.into());
    let mut edit = snapshot.edit_as(generation.clone());
    edit.observe_subtree(page)?;
    let mut observed = Vec::new();
    let mut contents = BTreeSet::new();
    if let Some(group) = snapshot.page(page)?.text_group()? {
        for layer in group.text_layers()? {
            if !layers.contains(&layer.id()) {
                continue;
            }
            let Some(line) = context.lines.get(&layer.id()) else {
                continue;
            };
            let Some(correction) = &line.ocr_correction else {
                continue;
            };
            let content = layer.content()?;
            let source = content.source()?;
            if correction_status(source.as_ref(), correction) != OcrCorrectionStatus::Pending
                || line.other.get(EVIDENCE) != Some(&evidence(&snapshot, layer.id(), page)?)
                || !contents.insert(content.id())
            {
                continue;
            }
            let mut source = source.context("OCR source disappeared")?;
            source.text = Authored::generated(correction.suggested.clone(), generation.clone());
            edit.set(content.id(), &source)?;
            let translation = content.translation()?.map(serde_json::to_value).transpose()?;
            observed.push((layer.id(), translation));
        }
    }
    if observed.is_empty() {
        return Ok(None);
    }
    let patch = edit.finish()?;
    // Persist the comparison baseline before the Scene edit. It is inert until
    // the correction is applied, and the original suggestion remains recoverable.
    project.update_context(|context| {
        for (layer, translation) in &observed {
            let line = context.lines.entry(*layer).or_default();
            if let Some(translation) = translation {
                line.other
                    .insert(TRANSLATION_BEFORE.into(), translation.clone());
            } else {
                line.other.remove(TRANSLATION_BEFORE);
            }
        }
    })?;
    let commit = project.commit(patch).await?;
    project.record_commit(&commit);
    Ok(Some((commit, observed.len() as u32)))
}

pub(crate) async fn accept(
    handle: &AppHandle<CefRuntime>,
    scope: &Scope,
) -> Result<OcrCorrectionResult> {
    let projects = handle.state::<CurrentProject>();
    let mut projects = projects.project.lock().await;
    let project = projects.as_mut().context("no project is open")?;
    let pending: Vec<_> = suggestions(project, scope)?
        .suggestions
        .into_iter()
        .filter(|suggestion| suggestion.status == OcrCorrectionStatus::Pending)
        .collect();
    let pages: Vec<_> = project.snapshot().pages().map(|page| page.id()).collect();
    let mut result = OcrCorrectionResult::default();
    for page in pages {
        let layers: Vec<_> = pending
            .iter()
            .filter(|row| row.page == page)
            .map(|row| row.layer)
            .collect();
        if layers.is_empty() {
            continue;
        }
        if let Some((commit, count)) = apply_page(project, page, &layers).await? {
            result.applied += count;
            result.pages += 1;
            let desktop = handle.state::<Desktop>();
            desktop
                .synchronize(&commit.snapshot, project.active_page(), &commit)
                .await?;
            handle
                .state::<CanvasChannel>()
                .channel
                .publish(desktop.canvas_state());
        }
    }
    Ok(result)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_ocr_suggestions(
    scope: Scope,
    project: State<'_, CurrentProject>,
) -> std::result::Result<OcrSuggestions, Error> {
    let projects = project.project.lock().await;
    Ok(suggestions(
        projects.as_ref().context("no project is open")?,
        &scope,
    )?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn accept_ocr_correction(
    layer: EntityId,
    handle: AppHandle<CefRuntime>,
) -> std::result::Result<OcrCorrectionResult, Error> {
    Ok(accept(&handle, &Scope::Entities(vec![layer])).await?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn dismiss_ocr_correction(
    layer: EntityId,
    project: State<'_, CurrentProject>,
) -> std::result::Result<(), Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    project.snapshot().text_layer(layer)?;
    project.update_context(|context| {
        if let Some(correction) = context
            .lines
            .get_mut(&layer)
            .and_then(|line| line.ocr_correction.as_mut())
        {
            correction.dismissed = true;
        }
    })?;
    Ok(())
}

/// Recipe entry point: call after Read chapter and before the translation stage.
#[tauri::command]
#[specta::specta]
pub(crate) async fn apply_ocr_corrections(
    handle: AppHandle<CefRuntime>,
    scope: Scope,
) -> std::result::Result<OcrCorrectionResult, Error> {
    let settings = super::context::settings::get_context_settings()?;
    if !settings.auto_apply_ocr_corrections {
        return Ok(OcrCorrectionResult {
            skip_reason: Some(OcrCheckSkipReason::Disabled),
            ..Default::default()
        });
    }
    if !super::context::reading::get_reading_model()?.vision {
        let projects = handle.state::<CurrentProject>();
        let projects = projects.project.lock().await;
        projects
            .as_ref()
            .context("no project is open")?
            .update_context(|context| {
                context.other.insert(
                    "ocr_check_skip_reason".into(),
                    serde_json::json!("no_vision_model"),
                );
            })?;
        return Ok(OcrCorrectionResult {
            skip_reason: Some(OcrCheckSkipReason::NoVisionModel),
            ..Default::default()
        });
    }
    Ok(accept(&handle, &scope).await?)
}

#[cfg(test)]
mod tests;
