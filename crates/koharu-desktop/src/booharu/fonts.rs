//! The font picker's Booharu parts: the favourite families, kept in
//! `[booharu-fonts]` and changed one at a time by their own command, not by
//! saving every setting, so two changes never undo each other, and the
//! names each family goes by in other languages.

use std::collections::BTreeMap;

use anyhow::ensure;
use booharu_core::settings::FontSettings;

use crate::commands::Result;

/// The favourite font families, sorted.
#[tauri::command]
#[specta::specta]
pub fn booharu_get_font_favorites() -> Result<Vec<String>> {
    Ok(koharu_config::get::<FontSettings>()?
        .favorites
        .into_iter()
        .collect())
}

/// Makes `family` a favourite, or with `favorite` false no longer one, in
/// one read and write of `[booharu-fonts]` under the settings file's lock,
/// and answers the favourites as saved, sorted.
#[tauri::command]
#[specta::specta]
pub async fn booharu_set_font_favorite(family: String, favorite: bool) -> Result<Vec<String>> {
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        ensure!(
            !family.trim().is_empty(),
            "a favourite font needs a family name"
        );
        let mut favorites = Vec::new();
        koharu_config::update(|fonts: &mut FontSettings| {
            if favorite {
                fonts.favorites.insert(family);
            } else {
                fonts.favorites.remove(&family);
            }
            favorites = fonts.favorites.iter().cloned().collect();
        })?;
        Ok(favorites)
    })
    .await??)
}

/// Each font family with names in other languages, by the name
/// `list_fonts` lists it by, with those names, such as 游ゴシック for Yu
/// Gothic: the installed families', and the bundled ones' whose faces are
/// on disk already, as this never downloads one. Worked out off the UI
/// thread; an installed family's names are read once per run.
#[tauri::command]
#[specta::specta]
pub async fn booharu_get_font_local_names() -> Result<BTreeMap<String, Vec<String>>> {
    Ok(koharu_renderer::booharu::local_names().await?)
}
