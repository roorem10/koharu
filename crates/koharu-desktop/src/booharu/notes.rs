//! A reviewer's note on a block or a text layer, kept in its Booharu
//! attributes as `review_note`.

use std::sync::Arc;

use koharu_scene::{Change, LayerId};
use tauri::State;

use super::apply;
use crate::{app::App, commands::Result};

/// Sets the reviewer's note on `layer`, a block or a text layer, to `note`
/// as one undo step, or removes it when `note` is null or blank. A locked
/// layer, or one inside a locked layer, refuses it.
#[tauri::command]
#[specta::specta]
pub async fn booharu_set_review_note(
    app: State<'_, Arc<App>>,
    layer: LayerId,
    note: Option<String>,
) -> Result<Change> {
    apply(&app, move |session| {
        booharu::boxes::review_note(&session.project(), layer, note)
    })
    .await
}
