//! Join and split text boxes that share a bubble.

use koharu_pipeline::BoxSettings;

use super::Error;

#[tauri::command]
#[specta::specta]
pub(crate) fn get_box_settings() -> Result<BoxSettings, Error> {
    Ok(koharu_config::load::<BoxSettings>("boxes")?.read()?.clone())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn save_box_settings(settings: BoxSettings) -> Result<BoxSettings, Error> {
    let config = koharu_config::load::<BoxSettings>("boxes")?;
    *config.write()? = settings.clone();
    config.save()?;
    Ok(settings)
}
