//! WO-13: export names, script export and import, project library and models folder.

pub(super) mod names;
pub(crate) mod scripts;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::Error;

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct LibrarySettings {
    pub export_name_pattern: String,
}

impl Default for LibrarySettings {
    fn default() -> Self {
        Self {
            export_name_pattern: "{index}_{page}".to_owned(),
        }
    }
}

pub(super) fn load_settings() -> anyhow::Result<LibrarySettings> {
    Ok(koharu_config::load::<LibrarySettings>("library")?.read()?.clone())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn get_library_settings() -> Result<LibrarySettings, Error> {
    Ok(load_settings()?)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn save_library_settings(settings: LibrarySettings) -> Result<LibrarySettings, Error> {
    names::validate_pattern(&settings.export_name_pattern)?;
    let config = koharu_config::load::<LibrarySettings>("library")?;
    let mut write = config.write()?;
    *write = settings.clone();
    write.save()?;
    Ok(settings)
}
