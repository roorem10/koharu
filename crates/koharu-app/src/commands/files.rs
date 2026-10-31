//! WO-13: export names, script export and import, project library and models folder.

pub(crate) mod library;
pub(super) mod names;
pub(crate) mod scripts;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::Error;

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct LibrarySettings {
    pub export_name_pattern: String,
    pub projects_folder: Option<PathBuf>,
}

impl Default for LibrarySettings {
    fn default() -> Self {
        Self {
            export_name_pattern: "{index}_{page}".to_owned(),
            projects_folder: None,
        }
    }
}

pub(super) fn load_settings() -> anyhow::Result<LibrarySettings> {
    Ok(koharu_config::load::<LibrarySettings>("library")?
        .read()?
        .clone())
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
    if let Some(path) = &settings.projects_folder {
        prepare_folder(path)?;
    }
    let config = koharu_config::load::<LibrarySettings>("library")?;
    let mut write = config.write()?;
    *write = settings.clone();
    write.save()?;
    Ok(settings)
}

pub(crate) fn prepare_folder(path: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(path.is_absolute(), "folder must be an absolute path");
    std::fs::create_dir_all(path)?;
    anyhow::ensure!(path.is_dir(), "the selected path is not a folder");
    Ok(())
}
