use anyhow::Result;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::commands::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OcrBoxMode {
    Read,
    #[default]
    ReadTranslate,
    ReadTranslateClean,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct OcrBoxSettings {
    pub default_mode: OcrBoxMode,
    pub subtitle_font_family: Option<String>,
    pub subtitle_size: f32,
    pub subtitle_fill: [u8; 4],
    pub subtitle_outline: [u8; 4],
}

impl Default for OcrBoxSettings {
    fn default() -> Self {
        Self {
            default_mode: OcrBoxMode::default(),
            subtitle_font_family: None,
            subtitle_size: 18.,
            subtitle_fill: [0, 0, 0, 255],
            subtitle_outline: [255; 4],
        }
    }
}

impl OcrBoxSettings {
    pub(super) fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.subtitle_size.is_finite() && (1. ..=4096.).contains(&self.subtitle_size),
            "subtitle font size must be between 1 and 4096 pixels"
        );
        anyhow::ensure!(
            !self.subtitle_font_family.as_ref().is_some_and(|font| {
                font.trim().is_empty() || font.len() > 4096 || font.contains('\0')
            }),
            "invalid subtitle font family"
        );
        Ok(())
    }

    pub(super) fn load() -> Result<Self> {
        let settings = koharu_config::load::<Self>("ocr_box")?.read()?.clone();
        settings.validate()?;
        Ok(settings)
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) fn get_ocr_box_settings() -> Result<OcrBoxSettings, Error> {
    Ok(OcrBoxSettings::load()?)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn save_ocr_box_settings(settings: OcrBoxSettings) -> Result<OcrBoxSettings, Error> {
    settings.validate()?;
    let config = koharu_config::load::<OcrBoxSettings>("ocr_box")?;
    let mut current = config.write()?;
    *current = settings.clone();
    current.save()?;
    Ok(settings)
}
