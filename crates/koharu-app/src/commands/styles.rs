use anyhow::Context as _;
use koharu_renderer::TextStyles;
use koharu_scene::{EntityId, Typography};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use super::project::Project;
use super::{Error, project::CurrentProject};

#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StyleScope {
    Layers(Vec<EntityId>),
    Page(EntityId),
    Project,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Type)]
pub struct StyleApplyResult {
    pub changed: u32,
    pub skipped: u32,
    pub kept_manual: u32,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Type)]
pub struct StyleTargetCounts {
    pub restylable: u32,
    pub manual: u32,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn count_text_style_targets(
    scope: StyleScope,
    project: State<'_, CurrentProject>,
) -> Result<StyleTargetCounts, Error> {
    let project = project.project.lock().await;
    let project = project.as_ref().context("no project is open")?;
    let snapshot = project.snapshot();
    let mut counts = StyleTargetCounts::default();
    for layer in Project::style_targets(&snapshot, scope)? {
        if snapshot
            .component::<Typography>(layer)?
            .is_some_and(|t| t.origin == koharu_scene::Origin::User)
        {
            counts.manual += 1;
        } else {
            counts.restylable += 1;
        }
    }
    Ok(counts)
}

pub(crate) fn load_text_styles() -> anyhow::Result<koharu_config::Config<TextStyles>> {
    TextStyles::load(|| {
        let pipeline = koharu_pipeline::PipelineConfig::load()?;
        let color = pipeline.read()?.default_text_color;
        Ok(color)
    })
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_text_styles() -> Result<TextStyles, Error> {
    Ok(load_text_styles()?.read()?.clone())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_text_styles(mut styles: TextStyles) -> Result<TextStyles, Error> {
    styles.validate()?;
    styles.legacy_color_migrated = true;
    let config = load_text_styles()?;
    let mut current = config.write()?;
    *current = styles.clone();
    current.save()?;
    Ok(styles)
}
