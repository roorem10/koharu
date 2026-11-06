use serde::{Deserialize, Serialize};
use specta::Type;

use super::Error;

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct CanvasBackgroundColors {
    pub light: [u8; 3],
    pub dark: [u8; 3],
}

impl Default for CanvasBackgroundColors {
    fn default() -> Self {
        Self {
            light: [255; 3],
            dark: [0; 3],
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CanvasBackgroundTheme {
    Light,
    Dark,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_canvas_backgrounds() -> Result<CanvasBackgroundColors, Error> {
    let config = koharu_config::load::<CanvasBackgroundColors>("canvas_backgrounds")?;
    Ok(config.read()?.clone())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_canvas_background(
    theme: CanvasBackgroundTheme,
    color: [u8; 3],
) -> Result<CanvasBackgroundColors, Error> {
    let config = koharu_config::load::<CanvasBackgroundColors>("canvas_backgrounds")?;
    let mut preferences = config.write()?;
    match theme {
        CanvasBackgroundTheme::Light => preferences.light = color,
        CanvasBackgroundTheme::Dark => preferences.dark = color,
    }
    let saved = preferences.clone();
    preferences.save()?;
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::CanvasBackgroundColors;

    #[test]
    fn rejects_invalid_color_channels() {
        for colors in [
            r#"{"light":[256,0,0]}"#,
            r#"{"dark":[-1,0,0]}"#,
            r#"{"light":[1.5,0,0]}"#,
            r#"{"dark":[1,2,3,4]}"#,
            r##"{"light":"#123456"}"##,
        ] {
            assert!(serde_json::from_str::<CanvasBackgroundColors>(colors).is_err());
        }
    }
}
