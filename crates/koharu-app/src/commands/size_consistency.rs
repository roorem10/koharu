mod calculation;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result, ensure};
use booharu_context::LetteringCategory;
use koharu_desktop::Desktop;
use koharu_pipeline::bubble_shape::text_bubble_shape;
use koharu_renderer::{
    AutoFitSizeLimit, LayerKind, RenderDiagnostic, TypesettingConfig, set_auto_fit_size_limit,
};
use koharu_scene::{Commit, EntityId, Patch, Snapshot, TextLayoutKind, TextRuns, Typography};
use serde::Serialize;
use specta::Type;
use tauri::State;

use super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    lettering::{
        resolve_category,
        sizes::{SizePolicy, has_manual_size},
    },
    project::{CurrentProject, Project},
};
use calculation::{SizeSample, consistency_targets, fitting_improvement};

#[derive(Clone, Debug, Default, Serialize, Type)]
pub(crate) struct SizeConsistencyResult {
    pub changed: u32,
    pub skipped: u32,
    pub kept_manual: u32,
    /// Chapter median as a fraction of page height, for the localized reason.
    pub median_fraction: Option<f32>,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn make_sizes_consistent(
    pages: Option<Vec<EntityId>>,
    project: State<'_, CurrentProject>,
    desktop: State<'_, Desktop>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<SizeConsistencyResult, Error> {
    Ok(run(pages, &project, &desktop, &canvas_channel).await?)
}

/// Shared entry point for the page/chapter UI, agent and future auto-run recipe.
/// Measurements never hold the project lock; a hand edit invalidates the work.
pub(crate) async fn run(
    pages: Option<Vec<EntityId>>,
    project: &CurrentProject,
    desktop: &Desktop,
    canvas_channel: &CanvasChannel,
) -> Result<SizeConsistencyResult> {
    let (snapshot, context) = {
        let current = project.project.lock().await;
        let project = current.as_ref().context("no project is open")?;
        (project.snapshot(), project.load_context()?)
    };
    let selected = match pages {
        Some(pages) => {
            for &page in &pages {
                snapshot.page(page)?;
            }
            pages.into_iter().collect::<BTreeSet<_>>()
        }
        None => snapshot.pages().map(|page| page.id()).collect(),
    };
    let mut result = SizeConsistencyResult::default();
    if selected.is_empty() {
        return Ok(result);
    }
    let policy = SizePolicy::load()?;
    let typesetting = TypesettingConfig::load()?;
    let font_revision = typesetting.revision();
    let renderer = desktop.renderer();
    let mut rows = Vec::new();
    let mut samples = Vec::new();
    // Even a page-only pass uses the whole chapter's usual dialogue size.
    for page in snapshot.pages() {
        let height = page.page()?.height as f32;
        let mut candidates = Vec::new();
        for layer in page.text_layers()? {
            let typography = layer.typography()?;
            let runs = snapshot.component::<TextRuns>(layer.id())?;
            if has_manual_size(typography.as_ref(), runs.as_ref()) {
                if selected.contains(&page.id()) {
                    result.kept_manual += 1;
                }
                continue;
            }
            let Some(typography) = typography else {
                continue;
            };
            let translated = layer
                .content()?
                .translation()?
                .is_some_and(|translation| !translation.text.value.trim().is_empty());
            if !translated || layer.layout()?.kind == TextLayoutKind::Point {
                if selected.contains(&page.id()) {
                    result.skipped += 1;
                }
                continue;
            }
            let info = text_bubble_shape(&snapshot, layer.id())?;
            if resolve_category(&context, &snapshot, layer.id(), &info, &policy.settings)?.category
                == LetteringCategory::NormalDialogue
            {
                candidates.push((layer.id(), typography));
            }
        }
        if candidates.is_empty() {
            continue;
        }
        let frame = renderer.render(&snapshot, page.id()).await?;
        for (layer, typography) in candidates {
            let Some(rendered) = frame.layer(layer) else {
                if selected.contains(&page.id()) {
                    result.skipped += 1;
                }
                continue;
            };
            let LayerKind::Text(text) = rendered.kind() else {
                continue;
            };
            samples.push(SizeSample {
                font_size: text.font_size,
                page_height: height,
            });
            rows.push((page.id(), layer, typography, height));
        }
    }
    let (median, targets) = consistency_targets(&samples);
    result.median_fraction = median;
    let mut updates = BTreeMap::<EntityId, Vec<(EntityId, Typography)>>::new();
    let mut measurements = BTreeMap::new();
    for (((page, layer, original, height), target), sample) in
        rows.into_iter().zip(targets).zip(samples)
    {
        if !selected.contains(&page) {
            continue;
        }
        let Some(target) = target else {
            continue;
        };
        let category_limit = policy.limit(LetteringCategory::NormalDialogue, height);
        let maximum = (target * height)
            .min(category_limit.map_or(f32::MAX, |limit| limit.maximum))
            .max(policy.minimum);
        let mut typography = original.clone();
        // The shared layout solver may shrink toward the QC floor if the
        // median cannot fit. No semantic text or word styles are rewritten.
        set_auto_fit_size_limit(
            &mut typography,
            Some(AutoFitSizeLimit {
                maximum,
                minimum: policy.minimum,
            }),
        )?;
        if typography != original {
            measurements.insert(layer, (sample.font_size, target * height));
            updates.entry(page).or_default().push((layer, typography));
        }
    }
    // A new QC floor must not make an older, smaller fitted box overflow.
    // Preview through the same solver; wrapping/contours are not monotonic.
    for (&page, changes) in &mut updates {
        let patch = page_patch(&snapshot, page, changes)?;
        let preview = snapshot.preview([&patch])?;
        let frame = renderer.render(&preview, page).await?;
        changes.retain(|(layer, _)| {
            let overflow = frame.diagnostics().iter().any(|diagnostic| {
                matches!(diagnostic, RenderDiagnostic::TextOverflow { entity, .. } if entity == layer)
            });
            let improves = frame.layer(*layer).is_some_and(|rendered| {
                let LayerKind::Text(text) = rendered.kind() else {
                    return false;
                };
                let (before, target) = measurements[layer];
                fitting_improvement(before, target, text.font_size, overflow)
            });
            if !improves {
                result.skipped += 1;
            }
            improves
        });
    }
    let mut expected = snapshot;
    // Commit in project order. Each page records its own inverse/undo entry.
    let order = expected.pages().map(|page| page.id()).collect::<Vec<_>>();
    for page in order {
        let Some(updates) = updates.remove(&page) else {
            continue;
        };
        if updates.is_empty() {
            continue;
        }
        let (commit, active) = {
            let mut current = project.project.lock().await;
            let project = current.as_mut().context("no project is open")?;
            ensure!(
                project.load_context()? == context,
                "lettering categories changed during size consistency"
            );
            let latest_policy = SizePolicy::load()?;
            ensure!(
                latest_policy.settings == policy.settings
                    && latest_policy.minimum == policy.minimum
                    && typesetting.revision() == font_revision,
                "typesetting settings changed during size consistency"
            );
            let commit = commit_page(project, &expected, page, &updates).await?;
            (commit, project.active_page())
        };
        result.changed += u32::try_from(updates.len())?;
        expected = commit.snapshot.clone();
        desktop.synchronize(&commit.snapshot, active, &commit).await?;
        canvas_channel.channel.publish(desktop.canvas_state());
    }
    Ok(result)
}

async fn commit_page(
    project: &mut Project,
    expected: &Snapshot,
    page: EntityId,
    updates: &[(EntityId, Typography)],
) -> Result<Commit> {
    let current = project.snapshot();
    ensure!(
        current.project_id() == expected.project_id() && current.revision() == expected.revision(),
        "project changed during size consistency; run it again"
    );
    let patch = page_patch(expected, page, updates)?;
    let commit = project.commit(patch).await?;
    project.record_commit(&commit);
    Ok(commit)
}

fn page_patch(
    expected: &Snapshot,
    page: EntityId,
    updates: &[(EntityId, Typography)],
) -> Result<Patch> {
    let members = expected
        .page(page)?
        .text_layers()?
        .map(|layer| layer.id())
        .collect::<BTreeSet<_>>();
    ensure!(
        updates.iter().all(|(layer, _)| members.contains(layer)),
        "size update belongs to another page"
    );
    Ok(expected.patch(|edit| {
        // The median observed every page. Keep those preconditions explicit
        // even though the current command also requires an exact revision.
        for page in expected.pages() {
            edit.observe_subtree(page.id())?;
        }
        for (layer, typography) in updates {
            edit.set_typography_preserving_origin(*layer, typography)?;
        }
        Ok(())
    })?)
}
