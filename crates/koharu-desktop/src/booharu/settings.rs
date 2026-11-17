//! Booharu's settings: every `[booharu-*]` section of `config.toml`, read and
//! saved whole, as Koharu's preferences commands do Koharu's sections.

use booharu_core::settings::BooharuPreferences;

use crate::commands::Result;

/// Every Booharu settings section, as the file holds it now.
#[tauri::command]
#[specta::specta]
pub fn booharu_get_preferences() -> Result<BooharuPreferences> {
    Ok(BooharuPreferences::get()?)
}

/// Saves every Booharu settings section, or none when one holds a bad value,
/// and answers them as the file holds them now.
#[tauri::command]
#[specta::specta]
pub async fn booharu_save_preferences(
    preferences: BooharuPreferences,
) -> Result<BooharuPreferences> {
    Ok(tokio::task::spawn_blocking(move || preferences.save()).await??)
}
