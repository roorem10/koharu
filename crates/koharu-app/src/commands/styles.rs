use anyhow::Context as _;
use koharu_renderer::{StyleGroup, TextStyle, TextStyles};
use koharu_scene::{EntityId, Typography};
use tauri::State;

use super::{Error, project::CurrentProject};

pub(crate) fn load_text_styles() -> anyhow::Result<koharu_config::Config<TextStyles>> {
    let pipeline = koharu_pipeline::PipelineConfig::load()?;
    let color = pipeline.read()?.default_text_color;
    TextStyles::load(color)
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

#[tauri::command]
#[specta::specta]
pub(crate) async fn capture_text_style(
    layer: EntityId,
    groups: Vec<StyleGroup>,
    project: State<'_, CurrentProject>,
) -> Result<TextStyle, Error> {
    let project = project.project.lock().await;
    let project = project.as_ref().context("no project is open")?;
    let snapshot = project.snapshot();
    snapshot.text_layer(layer)?;
    let typography = snapshot
        .component::<Typography>(layer)?
        .context("text layer has no typography")?;
    Ok(TextStyle::capture(&typography, &groups))
}
