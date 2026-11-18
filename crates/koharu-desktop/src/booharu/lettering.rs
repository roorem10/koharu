//! The lettering category commands: changing blocks' categories as the user
//! picks one, and each block's category on a page, for the Story panel, the
//! Layers panel, and Box details, each worked out by `booharu::lettering`,
//! which the agent's tools share, under the saved `[booharu-lettering]` and
//! the first of Koharu's Preferred fonts as they are when called.

use std::sync::Arc;

use booharu::lettering::{self, BlockLettering, CategoryCounts, Chooser};
use booharu_core::{lettering::LetteringCategory, settings::LetteringSettings};
use koharu_scene::{LayerId, PageId};
use tauri::State;

use crate::{app::App, booharu::styles::look, commands::Result};

/// Sets the lettering category of `blocks` as the user picks it, or
/// Automatic for none, which clears their labels, as one undo step labelled
/// `booharu_set_lettering_category`: each block's label, and while Pick fonts
/// automatically is on, its translation's category font, even one changed
/// by hand. Locked blocks, and blocks in locked layers, are left out.
/// Answers how many blocks it changed, left out as locked, and found
/// untranslated, which take only their label; with nothing to change, it
/// makes no undo step.
#[tauri::command]
#[specta::specta]
pub async fn booharu_set_lettering_category(
    app: State<'_, Arc<App>>,
    blocks: Vec<LayerId>,
    category: Option<LetteringCategory>,
) -> Result<CategoryCounts> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let (settings, font) = look()?;
        let session = app.session()?;
        let mut session = session.lock();
        let (edit, counts) = lettering::set_category(
            &session.project(),
            &blocks,
            category,
            Chooser::User,
            &settings,
            font.as_deref(),
        )?;
        if let Some(edit) = edit {
            tracing::info!(label = %edit.label, ops = edit.ops.len(), "edit");
            session.apply(edit)?;
        }
        Ok(counts)
    })
    .await??)
}

/// The lettering of every block on `page`, in reading order: the category
/// it's lettered in and why, its label, its balloon's shape with what
/// decided it, none outside any balloon, and whether its text or style was
/// changed by hand.
#[tauri::command]
#[specta::specta]
pub async fn booharu_get_lettering(
    app: State<'_, Arc<App>>,
    page: PageId,
) -> Result<Vec<BlockLettering>> {
    let project = app.session()?.lock().project();
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let settings = koharu_config::get::<LetteringSettings>()?;
        lettering::info(&project, page, &settings)
    })
    .await??)
}
