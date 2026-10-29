use anyhow::Context as _;
use koharu_desktop::Desktop;
use koharu_renderer::{StyleGroup, TextStyle, TextStyles};
use koharu_scene::{EntityId, Typography};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use super::{ChannelExt as _, canvas::CanvasChannel, project::Project};
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
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn count_text_style_targets(
    scope: StyleScope,
    project: State<'_, CurrentProject>,
) -> Result<u32, Error> {
    let project = project.project.lock().await;
    let project = project.as_ref().context("no project is open")?;
    Ok(u32::try_from(
        Project::style_targets(&project.snapshot(), scope)?.len(),
    )?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn apply_text_style(
    scope: StyleScope,
    style: TextStyle,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<StyleApplyResult, Error> {
    let (commit, page, result) = {
        let mut project = project.project.lock().await;
        let project = project.as_mut().context("no project is open")?;
        let (commit, result) = project.apply_text_style(scope, &style).await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, project.active_page(), result)
    };
    if let Some(commit) = commit {
        desktop.synchronize(&commit.snapshot, page, &commit).await?;
        canvas_channel.channel.publish(desktop.canvas_state());
    }
    Ok(result)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_text_bubble_shape(
    layer: EntityId,
    project: State<'_, CurrentProject>,
) -> Result<koharu_pipeline::bubble_shape::BubbleShapeInfo, Error> {
    let project = project.project.lock().await;
    let project = project.as_ref().context("no project is open")?;
    Ok(koharu_pipeline::bubble_shape::text_bubble_shape(
        &project.snapshot(),
        layer,
    )?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn apply_shape_rules(
    scope: StyleScope,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<StyleApplyResult, Error> {
    let styles = load_text_styles()?.read()?.clone();
    let detect_colors = koharu_pipeline::PipelineConfig::load()?
        .read()?
        .detect_text_color;
    let (commit, page, result) = {
        let mut project = project.project.lock().await;
        let project = project.as_mut().context("no project is open")?;
        let (commit, result) = project
            .apply_shape_rules(scope, &styles, detect_colors)
            .await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, project.active_page(), result)
    };
    if let Some(commit) = commit {
        desktop.synchronize(&commit.snapshot, page, &commit).await?;
        canvas_channel.channel.publish(desktop.canvas_state());
    }
    Ok(result)
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
    validate_styles(&styles)?;
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

fn validate_styles(styles: &TextStyles) -> anyhow::Result<()> {
    styles.validate()?;
    for preset in &styles.presets {
        anyhow::ensure!(
            preset
                .key
                .as_ref()
                .is_none_or(|key| super::preferences::valid_shortcut_binding(key)),
            "invalid text preset shortcut"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preset_shortcuts_use_the_tool_binding_grammar() {
        for (key, valid) in [
            ("1", true),
            ("ctrl+shift+1", true),
            ("alt+f2", true),
            ("ctrl+numpadadd", true),
            ("Ctrl+1", false),
            ("shift+ctrl+1", false),
            ("ctrl++", false),
            ("two", false),
        ] {
            let styles = TextStyles {
                presets: vec![koharu_renderer::TextPreset {
                    id: "test".into(),
                    name: "Test".into(),
                    style: TextStyle::default(),
                    key: Some(key.into()),
                }],
                ..Default::default()
            };
            assert_eq!(validate_styles(&styles).is_ok(), valid, "{key}");
        }
    }
}
