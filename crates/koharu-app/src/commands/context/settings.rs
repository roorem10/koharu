use koharu_translator::ModelSelection;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::super::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReadWhen {
    #[default]
    Missing,
    Always,
    Never,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct ContextSettings {
    #[serde(deserialize_with = "read_when")]
    #[specta(type = ReadWhen)]
    pub read_when: ReadWhen,
    pub send_images: bool,
    pub ask_before_reading: bool,
    pub read_model: Option<ModelSelection>,
    pub pages_per_request: u8,
    pub previous_pages: u8,
    pub agent_messages: u16,
}

impl Default for ContextSettings {
    fn default() -> Self {
        Self {
            read_when: ReadWhen::Missing,
            send_images: true,
            ask_before_reading: true,
            read_model: None,
            pages_per_request: 20,
            previous_pages: 1,
            agent_messages: 20,
        }
    }
}

fn read_when<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<ReadWhen, D::Error> {
    Ok(match ReadWhen::deserialize(deserializer)? {
        ReadWhen::Always => ReadWhen::Missing,
        value => value,
    })
}

pub(super) fn load() -> anyhow::Result<ContextSettings> {
    Ok(koharu_config::load::<ContextSettings>("chapter-context")?
        .read()?
        .clone())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn get_context_settings() -> Result<ContextSettings, Error> {
    Ok(load()?)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn save_context_settings(
    mut settings: ContextSettings,
) -> Result<ContextSettings, Error> {
    if !((1..=40).contains(&settings.pages_per_request)
        && settings.previous_pages <= 2
        && (1..=200).contains(&settings.agent_messages))
    {
        return Err(anyhow::anyhow!("invalid chapter context settings").into());
    }
    if let Some(model) = &settings.read_model
        && !super::reading::can_read_images(model)
    {
        return Err(anyhow::anyhow!("Read chapter needs a vision model").into());
    }
    if settings.read_when == ReadWhen::Always {
        settings.read_when = ReadWhen::Missing;
    }
    let config = koharu_config::load::<ContextSettings>("chapter-context")?;
    let mut write = config.write()?;
    *write = settings.clone();
    write.save()?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_keep_defaults_and_always_becomes_automatic_missing() {
        let settings: ContextSettings = serde_json::from_value(serde_json::json!({
            "read_when": "always", "send_images": false, "previous_pages": 2
        }))
        .unwrap();
        assert_eq!(settings.read_when, ReadWhen::Missing);
        assert!(settings.ask_before_reading);
        assert!(settings.read_model.is_none());
        assert_eq!(settings.previous_pages, 2);
        let empty: ContextSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, ContextSettings::default());
    }
}
