use anyhow::Result;
use koharu_pipeline::{OnPageFailure, RunOrder, RunPolicy, Stage};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::Error;

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct RunPreset {
    pub name: String,
    pub stages: Vec<Stage>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct RunSettings {
    pub retries: u8,
    pub on_page_failure: OnPageFailure,
    pub order: RunOrder,
    pub finish_sound: bool,
    pub presets: Vec<RunPreset>,
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            retries: 2,
            on_page_failure: OnPageFailure::ContinueWithOtherPages,
            order: RunOrder::Overlapped,
            finish_sound: true,
            presets: vec![
                RunPreset {
                    name: "Detect + OCR".into(),
                    stages: vec![Stage::Detection, Stage::Ocr],
                },
                RunPreset {
                    name: "Translate + clean up".into(),
                    stages: vec![Stage::Translation, Stage::Inpainting],
                },
            ],
        }
    }
}

impl RunSettings {
    pub(crate) fn load() -> Result<Self> {
        let settings = koharu_config::load::<Self>("runs")?.read()?.clone();
        settings.validate()?;
        Ok(settings)
    }

    fn validate(&self) -> Result<()> {
        require(self.retries <= 5, "retries must be between 0 and 5")?;
        require(
            self.presets.len() <= 100,
            "at most 100 run presets can be saved",
        )?;
        for preset in &self.presets {
            require(
                !preset.name.trim().is_empty()
                    && preset.name.len() <= 256
                    && !preset.name.contains('\0'),
                "invalid preset name",
            )?;
            require(
                !preset.stages.is_empty(),
                "a preset must select at least one stage",
            )?;
        }
        Ok(())
    }

    pub(crate) fn policy(&self) -> RunPolicy {
        RunPolicy {
            retries: self.retries,
            on_page_failure: self.on_page_failure,
            order: self.order,
        }
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_run_settings() -> std::result::Result<RunSettings, Error> {
    Ok(RunSettings::load()?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_run_settings(
    settings: RunSettings,
) -> std::result::Result<RunSettings, Error> {
    settings.validate()?;
    let config = koharu_config::load::<RunSettings>("runs")?;
    let mut value = config.write()?;
    *value = settings.clone();
    value.save()?;
    Ok(settings)
}

fn require(condition: bool, message: &str) -> Result<()> {
    anyhow::ensure!(condition, "{message}");
    Ok(())
}
