//! WO-11F: the Emphasis style per lettering category, AI emphasis, and the
//! agent tool that styles words.

use koharu_scene::{Edit, EntityId, Snapshot};

use super::Error;

/// Applies the box's lettering-category Emphasis style to `start..end`, as
/// ordinary word styles (one undo step). Used by the ★ button.
#[tauri::command]
#[specta::specta]
pub(crate) async fn apply_emphasis(layer: EntityId, start: u32, end: u32) -> Result<(), Error> {
    let _ = (layer, start, end);
    Err(anyhow::anyhow!("emphasis is not implemented yet (WO-11F)").into())
}

/// Hook called by WO-11A's `rich_text::after_translation`, inside its patch,
/// for the boxes the translation stage just rewrote. WO-11F: read the
/// translator's emphasis words for those boxes and add Emphasis runs.
#[allow(dead_code)] // Called by WO-11A's `after_translation`.
pub(crate) fn apply_hints(
    edit: &mut Edit,
    snapshot: &Snapshot,
    layers: &[EntityId],
) -> koharu_scene::Result<()> {
    let _ = (edit, snapshot, layers);
    Ok(())
}
