//! WO-11C: where each character of a text box is drawn, for the on-page
//! editor's caret and selection (WO-11D).

use anyhow::Context as _;
use koharu_desktop::Desktop;
use koharu_scene::EntityId;
use serde::Serialize;
use specta::Type;
use tauri::State;

use super::{Error, project::CurrentProject};

/// The drawn box of one character (or of one cluster, when several `char`s
/// form one glyph: then `end - start > 1`). Each line also ends with a
/// `start == end` caret box: zero width horizontally, zero height vertically.
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
pub(crate) async fn get_text_glyph_boxes(
    layer: EntityId,
    project: State<'_, CurrentProject>,
    desktop: State<'_, Desktop>,
) -> Result<Vec<GlyphBox>, Error> {
    let snapshot = project
        .project
        .lock()
        .await
        .as_ref()
        .context("no project is open")?
        .snapshot();
    snapshot.text_layer(layer)?;
    let mut page = layer;
    while let Some(parent) = snapshot.parent(page)? {
        page = parent;
    }
    snapshot.page(page)?;
    let frame = desktop.renderer().render(&snapshot, page).await?;
    let boxes = frame.text_glyph_boxes(layer).unwrap_or_default();
    // A late result must not install positions from an older edit in the on-page editor.
    let projects = project.project.lock().await;
    let current = projects.as_ref().context("no project is open")?.snapshot();
    if current.project_id() != snapshot.project_id() || current.revision() != snapshot.revision() {
        return Err(anyhow::anyhow!("project changed while reading glyph boxes").into());
    }
    Ok(boxes
        .into_iter()
        .map(|glyph| GlyphBox {
            start: glyph.start,
            end: glyph.end,
            line: glyph.line,
            quad: glyph.quad,
        })
        .collect())
}
