//! Booharu's settings: every `[booharu-*]` section of `config.toml`, read and
//! saved whole, as Koharu's preferences commands do Koharu's sections.

use std::sync::Arc;

use booharu_core::settings::BooharuPreferences;
use tauri::State;

use crate::{app::App, commands::Result};

/// Every Booharu settings section, as the file holds it now.
#[tauri::command]
#[specta::specta]
pub fn booharu_get_preferences() -> Result<BooharuPreferences> {
    Ok(BooharuPreferences::get()?)
}

/// Saves every Booharu settings section, or none when one of them holds a
/// bad value or the settings file can't be read, and answers them as the file
/// holds them now.
#[tauri::command]
#[specta::specta]
pub async fn booharu_save_preferences(
    app: State<'_, Arc<App>>,
    preferences: BooharuPreferences,
) -> Result<BooharuPreferences> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let saved = preferences.save()?;
        // The canvas takes the ones it reads, for the text made from now on.
        super::canvas::push(&app.screen);
        // Any section may change what the quality check finds, as
        // `[booharu-lettering]` regroups the odd font check's blocks.
        super::qc::touch_all();
        Ok(saved)
    })
    .await??)
}
