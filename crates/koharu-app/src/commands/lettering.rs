//! Resolve line categories without changing the scene's text or save format.

mod settings;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use anyhow::{Context as _, Result};
use booharu_context::{LabelSource, LetteringCategory, LineCategory, ProjectContext};
use koharu_pipeline::bubble_shape::{
    BubbleShape, BubbleShapeInfo, original_glyph_fraction, text_bubble_shape,
};
use koharu_renderer::{TextStyle, TextStyles};
use koharu_scene::{Commit, EntityId, Origin, Snapshot, Typography};
use serde::Serialize;
use specta::Type;
use tauri::State;

use super::{
    Error,
    project::{CurrentProject, Project},
    styles::StyleScope,
};
pub(super) use settings::LetteringSettings;
use settings::SfxMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CategoryReason {
    User,
    Model,
    SmallText,
    BubbleShape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
pub(crate) struct EffectiveCategory {
    pub category: LetteringCategory,
    pub reason: CategoryReason,
}

fn winning_label(
    label: Option<LineCategory>,
    settings: &LetteringSettings,
) -> Option<LineCategory> {
    use LetteringCategory::*;
    label.filter(|label| {
        label.source == LabelSource::User
            || matches!(label.value, Title | PhoneText | Sfx)
            || (matches!(
                label.value,
                Thoughts | WeakShout | Moan | Whisper | Shy | Crying | Menacing | Shout
            ) && label.confidence.unwrap_or(1.0) >= settings.model_confidence)
    })
}

fn effective_category(
    label: Option<LineCategory>,
    shape: Option<BubbleShape>,
    small: bool,
    settings: &LetteringSettings,
) -> LetteringCategory {
    use LetteringCategory::*;
    if let Some(label) = winning_label(label, settings) {
        return label.value;
    }
    if small {
        return SmallText;
    }
    match shape {
        Some(BubbleShape::Round) => NormalDialogue,
        Some(BubbleShape::Rectangle) => NarrationBox,
        Some(BubbleShape::Spiky) => Shout,
        None => Floating,
    }
}

pub(super) fn resolve_category(
    context: &ProjectContext,
    snapshot: &Snapshot,
    layer: EntityId,
    info: &BubbleShapeInfo,
    settings: &LetteringSettings,
) -> Result<EffectiveCategory> {
    let label = context.category(layer);
    let winner = winning_label(label, settings);
    let small = winner.is_none()
        && original_glyph_fraction(snapshot, layer)?
            .is_some_and(|fraction| fraction < f64::from(settings.small_text_threshold));
    let reason = if let Some(label) = winner {
        if label.source == LabelSource::User {
            CategoryReason::User
        } else {
            CategoryReason::Model
        }
    } else if small {
        CategoryReason::SmallText
    } else {
        CategoryReason::BubbleShape
    };
    Ok(EffectiveCategory {
        category: effective_category(label, info.shape, small, settings),
        reason,
    })
}

pub(crate) fn pick_style<'a>(
    styles: &'a TextStyles,
    context: &ProjectContext,
    snapshot: &Snapshot,
    layer: EntityId,
    info: &BubbleShapeInfo,
) -> Result<Option<&'a TextStyle>> {
    if !styles.shape_styles.enabled {
        return Ok(None);
    }
    let settings = LetteringSettings::load()?.read()?.clone();
    let category = resolve_category(context, snapshot, layer, info, &settings)?.category;
    Ok(Some(styles.category_style(category.as_str())))
}

pub(crate) fn translation_skip(
    context: &ProjectContext,
    snapshot: &Snapshot,
) -> Result<BTreeSet<EntityId>> {
    let settings = LetteringSettings::load()?.read()?.clone();
    translation_skip_with(context, snapshot, &settings)
}

fn translation_skip_with(
    context: &ProjectContext,
    snapshot: &Snapshot,
    settings: &LetteringSettings,
) -> Result<BTreeSet<EntityId>> {
    if settings.sfx == SfxMode::TranslateAll {
        return Ok(BTreeSet::new());
    }
    let mut skip = BTreeSet::new();
    for layer in snapshot.text_layers()? {
        // SFX is a presentation label, so size and shape cannot override it.
        if winning_label(context.category(layer.id()), settings)
            .is_none_or(|label| label.value != LetteringCategory::Sfx)
        {
            continue;
        }
        let info = text_bubble_shape(snapshot, layer.id())?;
        if settings.sfx == SfxMode::Skip || info.bubble.is_none() {
            skip.insert(layer.id());
        }
    }
    Ok(skip)
}

pub(crate) async fn labels_changed(
    project: &mut Project,
    layers: &[EntityId],
) -> Result<Option<Commit>> {
    let styles = super::styles::load_text_styles()?.read()?.clone();
    let settings = LetteringSettings::load()?.read()?.clone();
    let detect_colors = koharu_pipeline::PipelineConfig::load()?
        .read()?
        .detect_text_color;
    restyle_labels(project, layers, &styles, &settings, detect_colors).await
}

async fn restyle_labels(
    project: &mut Project,
    layers: &[EntityId],
    styles: &TextStyles,
    settings: &LetteringSettings,
    detect_colors: bool,
) -> Result<Option<Commit>> {
    if !styles.shape_styles.enabled {
        return Ok(None);
    }
    let context = project.load_context()?;
    let snapshot = project.snapshot();
    let mut targets = Vec::new();
    for &layer in layers {
        let model_change = context
            .category(layer)
            .is_some_and(|label| label.source == LabelSource::Model);
        if model_change
            && snapshot
                .component::<Typography>(layer)?
                .is_some_and(|t| t.origin == Origin::User)
        {
            continue;
        }
        // No label is an explicit return to Automatic, including for a manual box.
        targets.push(layer);
    }
    let (commit, _) = project
        .apply_style(
            StyleScope::Layers(targets),
            false,
            true,
            |snapshot, layer, typography| {
                let info = text_bubble_shape(snapshot, layer)?;
                let category =
                    resolve_category(&context, snapshot, layer, &info, settings)?.category;
                styles
                    .default_style
                    .apply_to_detected(typography, detect_colors);
                styles
                    .category_style(category.as_str())
                    .apply_to_detected(typography, detect_colors);
                Ok(())
            },
        )
        .await?;
    Ok(commit)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_effective_category(
    layer: EntityId,
    project: State<'_, CurrentProject>,
) -> Result<EffectiveCategory, Error> {
    let settings = LetteringSettings::load()?.read()?.clone();
    let project = project.project.lock().await;
    let project = project.as_ref().context("no project is open")?;
    let snapshot = project.snapshot();
    let info = text_bubble_shape(&snapshot, layer)?;
    Ok(resolve_category(
        &project.load_context()?,
        &snapshot,
        layer,
        &info,
        &settings,
    )?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_lettering_settings() -> Result<LetteringSettings, Error> {
    Ok(LetteringSettings::load()?.read()?.clone())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_lettering_settings(
    settings: LetteringSettings,
) -> Result<LetteringSettings, Error> {
    settings.validate()?;
    let config = LetteringSettings::load()?;
    let mut current = config.write()?;
    *current = settings.clone();
    current.save()?;
    Ok(settings)
}
