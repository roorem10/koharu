//! Resolve line categories without changing the scene's text or save format.

mod decision;
mod font_coverage;
mod settings;
pub(super) mod source;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result};
use booharu_context::{LabelSource, LetteringCategory, LineCategory, ProjectContext, SourceStyle};
use koharu_pipeline::bubble_shape::{
    BubbleShapeInfo, original_glyph_fraction, text_bubble_shape,
};
use koharu_renderer::{Renderer, TextStyles};
use koharu_scene::{Commit, EntityId, Origin, Snapshot, Typography};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use super::{
    Error,
    project::{CurrentProject, Project},
    styles::StyleScope,
};
use decision::decide;
pub(crate) use font_coverage::apply_font_coverage;
pub(super) use settings::LetteringSettings;
use settings::SfxMode;

const DECISION_KEY: &str = "dev.booharu.lettering";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CategoryReason {
    User,
    Model,
    SmallText,
    BubbleShape,
    SourceStyle,
    ShoutSignals,
    HandStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShoutSignal {
    SourceBold,
    SpikyBubble,
    YellingLine,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
pub(crate) struct EffectiveCategory {
    pub category: LetteringCategory,
    pub reason: CategoryReason,
    pub bold: bool,
    pub signals: Vec<ShoutSignal>,
    pub source_style: Option<SourceStyle>,
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

pub(super) fn resolve_category(
    context: &ProjectContext,
    snapshot: &Snapshot,
    layer: EntityId,
    info: &BubbleShapeInfo,
    settings: &LetteringSettings,
) -> Result<EffectiveCategory> {
    let label = context.category(layer);
    let winner = winning_label(label, settings);
    let small = winner.is_none_or(|label| {
        label.source == LabelSource::Model
            && matches!(
                label.value,
                LetteringCategory::Shout | LetteringCategory::WeakShout
            )
    })
        && original_glyph_fraction(snapshot, layer)?
            .is_some_and(|fraction| fraction < f64::from(settings.small_text_threshold));
    Ok(decide(
        label,
        info.shape,
        small,
        context.source_style(layer),
        settings,
    ))
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
    include_user: bool,
    renderer: &Renderer,
) -> Result<Option<Commit>> {
    let styles = super::styles::load_text_styles()?.read()?.clone();
    let settings = LetteringSettings::load()?.read()?.clone();
    let detect_colors = koharu_pipeline::PipelineConfig::load()?
        .read()?
        .detect_text_color;
    restyle_labels(
        project,
        layers,
        &styles,
        &settings,
        detect_colors,
        include_user,
        renderer,
    )
    .await
}

pub(crate) async fn after_analysis(
    project: &mut Project,
    page: EntityId,
    renderer: &Renderer,
) -> Result<Option<Commit>> {
    let layers = project
        .snapshot()
        .page(page)?
        .text_layers()?
        .map(|layer| layer.id())
        .collect::<Vec<_>>();
    let styles = super::styles::load_text_styles()?.read()?.clone();
    if !styles.shape_styles.enabled {
        source::refresh(project, &layers, true).await?;
        return Ok(None);
    }
    let settings = LetteringSettings::load()?.read()?.clone();
    let detect_colors = koharu_pipeline::PipelineConfig::load()?
        .read()?
        .detect_text_color;
    restyle_labels(
        project,
        &layers,
        &styles,
        &settings,
        detect_colors,
        false,
        renderer,
    )
    .await
}

async fn restyle_labels(
    project: &mut Project,
    layers: &[EntityId],
    styles: &TextStyles,
    settings: &LetteringSettings,
    detect_colors: bool,
    include_user: bool,
    renderer: &Renderer,
) -> Result<Option<Commit>> {
    if !styles.shape_styles.enabled {
        return Ok(None);
    }
    let snapshot = project.snapshot();
    let mut targets = Vec::new();
    for &layer in layers {
        if !include_user
            && snapshot
                .component::<Typography>(layer)?
                .is_some_and(|t| t.origin == Origin::User)
        {
            continue;
        }
        targets.push(layer);
    }
    if targets.is_empty() {
        source::refresh(project, layers, true).await?;
        return Ok(None);
    }
    let prepared = prepare_styles(project, &targets, styles, settings, detect_colors, renderer).await?;
    let (commit, _) = project
        .apply_style(
            StyleScope::Layers(targets),
            false,
            true,
            |_, layer, typography| {
                *typography = prepared
                    .get(&layer)
                    .context("lettering could not be prepared")?
                    .clone();
                Ok(())
            },
        )
        .await?;
    Ok(commit)
}

pub(crate) async fn prepare_styles(
    project: &mut Project,
    layers: &[EntityId],
    styles: &TextStyles,
    settings: &LetteringSettings,
    detect_colors: bool,
    renderer: &Renderer,
) -> Result<BTreeMap<EntityId, Typography>> {
    let context = source::refresh(project, layers, true).await?;
    let snapshot = project.snapshot();
    let mut prepared = BTreeMap::new();
    for &layer in layers {
        let info = text_bubble_shape(&snapshot, layer)?;
        if info.bubble.is_some() && info.measures.is_none() {
            continue;
        }
        let Some(typography) = snapshot.component::<Typography>(layer)? else {
            continue;
        };
        let decision = resolve_category(&context, &snapshot, layer, &info, settings)?;
        let translation = snapshot.text_layer(layer)?.content()?.translation()?;
        match prepare_typography(
            styles,
            &decision,
            typography,
            detect_colors,
            renderer,
            translation.as_ref(),
        )
        .await
        {
            Ok(typography) => {
                prepared.insert(layer, typography);
            }
            Err(error) => {
                tracing::warn!(%layer, %error, "lettering font cannot cover the translation");
            }
        }
    }
    Ok(prepared)
}

pub(super) async fn prepare_typography(
    styles: &TextStyles,
    decision: &EffectiveCategory,
    mut typography: Typography,
    detect_colors: bool,
    renderer: &Renderer,
    translation: Option<&koharu_scene::Translation>,
) -> Result<Typography> {
    // Font intent belongs to the current category decision. An empty preset
    // must not inherit a previous automatic Shout (including detection's preset).
    typography.preferred_font = None;
    typography.font_weight = None;
    typography.font_style = None;
    styles
        .default_style
        .apply_to_detected(&mut typography, detect_colors);
    styles
        .category_style(decision.category.as_str())
        .apply_to_detected(&mut typography, detect_colors);
    apply_source_weight(&mut typography, decision);
    if let Some(translation) = translation {
        apply_font_coverage(
            renderer,
            styles,
            decision.category,
            &mut typography,
            &translation.text.value,
            translation.language.as_ref().map(|language| language.as_str()),
        )
        .await?;
    }
    typography
        .extensions
        .insert(DECISION_KEY.into(), serde_json::to_string(decision)?);
    Ok(typography)
}

pub(super) fn apply_source_weight(typography: &mut Typography, decision: &EffectiveCategory) {
    if decision.bold {
        typography.font_weight = Some(typography.font_weight.unwrap_or(400).max(700));
    } else if decision.reason != CategoryReason::User
        && decision
            .source_style
            .is_some_and(|style| style.source_bold == booharu_context::SourceBold::No)
    {
        typography.font_weight = Some(400);
    }
}

pub(crate) async fn inspect_source_style(
    project: &Project,
    layer: EntityId,
) -> Result<EffectiveCategory> {
    let settings = LetteringSettings::load()?.read()?.clone();
    let snapshot = project.snapshot();
    let context = source::refresh(project, &[layer], false).await?;
    let info = text_bubble_shape(&snapshot, layer)?;
    let mut decision = resolve_category(&context, &snapshot, layer, &info, &settings)?;
    if let Some(typography) = snapshot.component::<Typography>(layer)? {
        if let Some(saved) = typography.extensions.get(DECISION_KEY) {
            decision = serde_json::from_str(saved)?;
            decision.source_style = context.source_style(layer);
        }
        if typography.origin == Origin::User {
            decision.reason = CategoryReason::HandStyle;
            decision.bold = typography.font_weight.unwrap_or(400) >= 700;
        }
    }
    Ok(decision)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_effective_category(
    layer: EntityId,
    project: State<'_, CurrentProject>,
) -> Result<EffectiveCategory, Error> {
    let project = project.project.lock().await;
    let project = project.as_ref().context("no project is open")?;
    Ok(inspect_source_style(project, layer).await?)
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
