//! WO-11C: where each character of a text box is drawn, for the on-page
//! editor's caret and selection (WO-11D).

use koharu_scene::EntityId;
use serde::Serialize;
use specta::Type;

use super::Error;

/// The drawn box of one character (or of one cluster, when several `char`s
/// form one glyph: then `end - start > 1`).
#[derive(Clone, Debug, Serialize, Type)]
pub(crate) struct GlyphBox {
    pub start: u32,
    pub end: u32,
    /// Line number, from 0, in reading order.
    pub line: u32,
    /// Corners in page coordinates, after rotation and skew/perspective:
    /// top-left, top-right, bottom-right, bottom-left in the glyph's own
    /// reading direction.
    pub quad: [[f32; 2]; 4],
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_text_glyph_boxes(layer: EntityId) -> Result<Vec<GlyphBox>, Error> {
    let _ = layer;
    Err(anyhow::anyhow!("glyph boxes are not implemented yet (WO-11C)").into())
}
