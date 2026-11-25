//! Typing a block's translation in the Story panel: a block without one of
//! its own, none or only the placeholder a style set before translating
//! leaves, gets its first translation where Koharu's scene starts one, in
//! its source's place (#1494), as the canvas types one, in the same undo
//! step as what Koharu's lacks: the translator's target language, its
//! clean-up shown, and the default look a new translation gets where the
//! placeholder set no value of its own.

use std::sync::Arc;

use booharu_core::settings::Snapshot;
use koharu_pipeline::PipelineConfig;
use koharu_renderer::RendererConfig;
use koharu_scene::{Change, LayerId};
use tauri::State;

use crate::{app::App, commands::Result};

/// Sets `block`'s translation to `text` as one undo step, and when the
/// block has none of its own, none or only the placeholder a style set
/// before translating leaves, whose values stay, writes it as a first
/// translation: in its source's place, in the translator's target language,
/// over its clean-up shown, with the default look.
#[tauri::command]
#[specta::specta]
pub async fn booharu_set_translation(
    app: State<'_, Arc<App>>,
    block: LayerId,
    text: String,
) -> Result<Change> {
    let app = Arc::clone(&app);
    // As `App::edit` applies an edit, but under the lock the edit is made
    // under, so a first translation is written only where the edit finds
    // none.
    Ok(
        tokio::task::spawn_blocking(move || -> anyhow::Result<Change> {
            let target = koharu_config::get::<PipelineConfig>()?.translator.target;
            // A new translation's default look, as Booharu's settings hold it
            // now; one that can't be read fails the edit.
            let families =
                koharu_config::get::<RendererConfig>().map(|config| config.font_families);
            let look = Snapshot::read(families)?;
            let session = app.session()?;
            let mut session = session.lock();
            let edit = booharu::translation::edit(&session.project(), block, text, &target, &look)?;
            tracing::info!(label = %edit.label, ops = edit.ops.len(), "edit");
            Ok(session.apply(edit)?)
        })
        .await??,
    )
}
