//! Typing a block's translation in the Story panel: a block without one gets
//! it placed into its own frame first, in the same undo step, where the
//! scene's `set_text` alone would leave it at the page's corner (#128).

use std::sync::Arc;

use koharu_pipeline::PipelineConfig;
use koharu_scene::{Change, LayerId};
use tauri::State;

use crate::{app::App, commands::Result};

/// Sets `block`'s translation to `text` as one undo step, placing a new one
/// as the Translation stage places its own, in the translator's target
/// language.
#[tauri::command]
#[specta::specta]
pub async fn booharu_set_translation(
    app: State<'_, Arc<App>>,
    block: LayerId,
    text: String,
) -> Result<Change> {
    let app = Arc::clone(&app);
    // As `App::edit` applies an edit, but under the lock the edit is made
    // under, so a translation is placed only where the edit finds none.
    Ok(
        tokio::task::spawn_blocking(move || -> anyhow::Result<Change> {
            let target = koharu_config::get::<PipelineConfig>()?.translator.target;
            let session = app.session()?;
            let mut session = session.lock();
            let edit = booharu::translation::edit(&session.project(), block, text, &target)?;
            tracing::info!(label = %edit.label, ops = edit.ops.len(), "edit");
            Ok(session.apply(edit)?)
        })
        .await??,
    )
}
