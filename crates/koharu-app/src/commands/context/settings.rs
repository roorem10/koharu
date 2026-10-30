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
    pub read_when: ReadWhen,
    pub send_images: bool,
    pub pages_per_request: u8,
    pub previous_pages: u8,
    pub agent_messages: u16,
}

impl Default for ContextSettings {
    fn default() -> Self {
        Self {
            read_when: ReadWhen::Missing,
            send_images: true,
            pages_per_request: 20,
            previous_pages: 1,
            agent_messages: 20,
        }
    }
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
pub(crate) fn save_context_settings(settings: ContextSettings) -> Result<ContextSettings, Error> {
    if !((1..=40).contains(&settings.pages_per_request)
        && settings.previous_pages <= 2
        && (1..=200).contains(&settings.agent_messages))
    {
        return Err(anyhow::anyhow!("invalid chapter context settings").into());
    }
    let config = koharu_config::load::<ContextSettings>("chapter-context")?;
    let mut write = config.write()?;
    *write = settings.clone();
    write.save()?;
    Ok(settings)
}
