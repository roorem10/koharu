//! WO-11A: word styles inside a text box (data rules and commands).
//!
//! The foundation fixes the command signatures below so WO-11D, 11E and 11F
//! can call them while WO-11A fills them in. Offsets are `char` offsets of
//! the text the box shows (see `koharu_scene::StyleRun`).

use koharu_scene::{Commit, Edit, EntityId, RunStyle, Snapshot, StyleRun};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::Error;

/// One field of [`RunStyle`], for clearing it on a range.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunStyleField {
    Font,
    FontWeight,
    FontStyle,
    SizeScale,
    Color,
    FillGradient,
    StrokeColor,
    StrokeWidth,
    OuterStrokeColor,
    OuterStrokeWidth,
}

/// A change to the word styles of a range: fields in `set` that are `Some`
/// are set, fields listed in `clear` go back to the box's own style.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
pub(crate) struct RunStylePatch {
    pub set: RunStyle,
    pub clear: Vec<RunStyleField>,
}

/// Sets or clears styles on `start..end` of one box, as one undo step.
#[tauri::command]
#[specta::specta]
pub(crate) async fn set_word_style(
    layer: EntityId,
    start: u32,
    end: u32,
    patch: RunStylePatch,
) -> Result<(), Error> {
    let _ = (layer, start, end, patch);
    Err(anyhow::anyhow!("word styles are not implemented yet (WO-11A)").into())
}

/// Removes word styles: on `start..end` of each box when given, otherwise
/// all of them ("Clear all word styles"). One undo step for all boxes.
#[tauri::command]
#[specta::specta]
pub(crate) async fn clear_word_styles(
    layers: Vec<EntityId>,
    range: Option<[u32; 2]>,
) -> Result<(), Error> {
    let _ = (layers, range);
    Err(anyhow::anyhow!("word styles are not implemented yet (WO-11A)").into())
}

/// Replaces `start..end` of the box's shown text with `insert` (typing on the
/// page), keeping word styles on their characters. Typed text takes the style
/// of the character before it. One undo step.
#[tauri::command]
#[specta::specta]
pub(crate) async fn replace_text_range(
    layer: EntityId,
    start: u32,
    end: u32,
    insert: String,
) -> Result<(), Error> {
    let _ = (layer, start, end, insert);
    Err(anyhow::anyhow!("word styles are not implemented yet (WO-11A)").into())
}

/// The box's word styles (empty when it has none), for the editor's and the
/// panel's mixed values. WO-11A fills it.
#[tauri::command]
#[specta::specta]
pub(crate) async fn get_word_styles(layer: EntityId) -> Result<Vec<StyleRun>, Error> {
    let _ = layer;
    Ok(Vec::new())
}

/// Hook called by `Project::set_translation` and `Project::set_source_text`
/// inside their patch, after the new text is set. `new_shown` is the text the
/// box shows after the edit (its translation, or its source without one).
/// WO-11A: move, shrink or drop the box's runs so they stay on their
/// characters. The foundation leaves the runs as they are.
pub(crate) fn on_text_edit(
    edit: &mut Edit,
    snapshot: &Snapshot,
    layer: EntityId,
    new_shown: &str,
) -> koharu_scene::Result<()> {
    let _ = (edit, snapshot, layer, new_shown);
    Ok(())
}

/// Hook called by the processing committer right after a Translation stage
/// commit (normal runs and Translate again). WO-11A: for every box whose
/// translation that commit rewrote, remove its `TextRuns` and reset its
/// `Typography` to the lettering-category default (decision 4), then call
/// `super::emphasis::apply_hints` in the same patch. Return the follow-up
/// commit, if any; the committer records and publishes it.
pub(crate) async fn after_translation(
    project: &mut super::project::Project,
    commit: &Commit,
) -> anyhow::Result<Option<Commit>> {
    let _ = (project, commit);
    Ok(None)
}
