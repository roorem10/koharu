//! Category emphasis is copied into ordinary, editable word styles.

pub(crate) mod agent;
mod runs;
#[cfg(test)]
mod tests;

use anyhow::Context as _;
use booharu_context::ProjectContext;
use koharu_desktop::Desktop;
use koharu_pipeline::{EmphasisHints, bubble_shape::text_bubble_shape};
use koharu_scene::{Edit, EntityId, Origin, RunStyle, Snapshot, TextRuns};
use koharu_translator::EmphasisSettings;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    project::{CurrentProject, Project},
};

/// IPC always includes the preset, while persisted configuration may omit its
/// baseline table to preserve cleared fields across config-default merging.
#[derive(Clone, Serialize, Deserialize, Type)]
pub(crate) struct EmphasisPreferences {
    pub enabled_for_ai: bool,
    pub default: RunStyle,
    pub categories: std::collections::BTreeMap<String, RunStyle>,
}

impl From<EmphasisSettings> for EmphasisPreferences {
    fn from(settings: EmphasisSettings) -> Self {
        Self {
            enabled_for_ai: settings.enabled_for_ai,
            default: settings.default,
            categories: settings.categories,
        }
    }
}

impl From<EmphasisPreferences> for EmphasisSettings {
    fn from(settings: EmphasisPreferences) -> Self {
        Self {
            enabled_for_ai: settings.enabled_for_ai,
            default: settings.default,
            categories: settings.categories,
        }
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_emphasis_settings() -> Result<EmphasisPreferences, Error> {
    Ok(EmphasisSettings::load()?.read()?.clone().into())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_emphasis_settings(
    settings: EmphasisPreferences,
) -> Result<EmphasisPreferences, Error> {
    let settings = EmphasisSettings::from(settings);
    settings.validate()?;
    if !settings.categories.keys().all(|key| {
        booharu_context::LetteringCategory::ALL
            .into_iter()
            .any(|category| category.as_str() == key)
    }) {
        return Err(anyhow::anyhow!("unknown emphasis lettering category").into());
    }
    let config = EmphasisSettings::load()?;
    let mut saved = config.write()?;
    *saved = settings.clone();
    saved.save()?;
    Ok(settings.into())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn apply_emphasis(
    layer: EntityId,
    start: u32,
    end: u32,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<(), Error> {
    let settings = EmphasisSettings::load()?.read()?.clone();
    let (commit, page) = {
        let mut projects = project.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let snapshot = project.snapshot();
        runs::validate_range(&shown_text(&snapshot, layer)?, start, end)?;
        let style = category_style(&settings, &project.load_context()?, &snapshot, layer)?;
        let patch = snapshot.patch(|edit| {
            runs::write(
                edit,
                &snapshot,
                layer,
                start,
                end,
                Some(&style),
                Origin::User,
            )
        })?;
        let commit = project.commit(patch).await?;
        project.record_commit(&commit);
        (commit, project.active_page())
    };
    desktop.synchronize(&commit.snapshot, page, &commit).await?;
    canvas_channel.channel.publish(desktop.canvas_state());
    Ok(())
}

fn shown_text(snapshot: &Snapshot, layer: EntityId) -> koharu_scene::Result<String> {
    let content = snapshot.text_layer(layer)?.content()?;
    Ok(if let Some(translation) = content.translation()? {
        translation.text.value
    } else {
        content
            .source()?
            .map(|source| source.text.value)
            .unwrap_or_default()
    })
}

fn category_style(
    settings: &EmphasisSettings,
    context: &ProjectContext,
    snapshot: &Snapshot,
    layer: EntityId,
) -> anyhow::Result<RunStyle> {
    let info = text_bubble_shape(snapshot, layer)?;
    let lettering = super::lettering::LetteringSettings::load()?.read()?.clone();
    let category = super::lettering::resolve_category(context, snapshot, layer, &info, &lettering)?;
    Ok(settings.style(category.category.as_str()))
}

/// The caller passes the sidecar it already owns: category labels aren't in the scene.
#[allow(dead_code)] // Called by WO-11A's after_translation.
pub(crate) fn apply_hints(
    edit: &mut Edit,
    snapshot: &Snapshot,
    layers: &[EntityId],
    context: &ProjectContext,
) -> koharu_scene::Result<()> {
    let settings = EmphasisSettings::load()
        .and_then(|config| Ok(config.read()?.clone()))
        .map_err(|error| koharu_scene::Error::Invalid(error.to_string()))?;
    apply_hints_with(edit, snapshot, layers, &settings, |layer| {
        category_style(&settings, context, snapshot, layer)
            .map_err(|error| koharu_scene::Error::Invalid(error.to_string()))
    })
}

fn apply_hints_with(
    edit: &mut Edit,
    snapshot: &Snapshot,
    layers: &[EntityId],
    settings: &EmphasisSettings,
    style: impl Fn(EntityId) -> koharu_scene::Result<RunStyle>,
) -> koharu_scene::Result<()> {
    for &layer in layers {
        let Some(hints) = snapshot.component::<EmphasisHints>(layer)? else {
            continue;
        };
        if settings.enabled_for_ai && !hints.words.is_empty() {
            let text = shown_text(snapshot, layer)?;
            let ranges = runs::find_words(&text, &hints.words);
            if !ranges.is_empty() {
                let style = style(layer)?;
                let mut runs = Vec::new();
                for (start, end) in ranges {
                    runs = runs::patched(&runs, start, end, Some(&style));
                }
                if !runs.is_empty() {
                    edit.set(
                        layer,
                        &TextRuns {
                            origin: hints.origin,
                            runs,
                        },
                    )?;
                }
            }
        }
        edit.remove::<EmphasisHints>(layer)?;
    }
    Ok(())
}
