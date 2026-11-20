//! The type bar's commands for the text being typed on the canvas: Done,
//! which commits it as Ctrl+Enter and Escape do, Clear, B off and I off,
//! which take marks off it, and A+ and A−, which step its size. Each goes
//! through the screen, which changes the canvas on the window's thread and
//! tells the interface the typing that results.

use std::sync::Arc;

use koharu_canvas::booharu::Unmark;
use tauri::State;

use crate::app::App;

/// Commits the text being typed, as Ctrl+Enter does, as one undo step, none
/// when nothing changed: the type bar's Done. Nothing when not typing.
#[tauri::command]
#[specta::specta]
pub fn booharu_finish_typing(app: State<'_, Arc<App>>) {
    app.screen.finish_typing(Arc::clone(&app));
}

/// Takes marks off the text being typed, as the type bar's Clear, B off and
/// I off ask: off the selected characters, or off the next ones typed at a
/// caret. `all` takes every mark; `bold` the weight, or sets regular where
/// the text itself is bold; `italic` the italic, or sets upright where the
/// text itself is italic. Nothing when not typing.
#[tauri::command]
#[specta::specta]
pub fn booharu_unmark_typing(app: State<'_, Arc<App>>, what: Unmark) {
    app.screen.unmark_typing(what);
}

/// The text being typed a step bigger, or smaller, 5% to the half pixel, as
/// the type bar's A+ and A− ask: each selected character from its own size,
/// else the text's, or at a caret the next ones typed. Nothing when not
/// typing.
#[tauri::command]
#[specta::specta]
pub fn booharu_step_typing_size(app: State<'_, Arc<App>>, bigger: bool) {
    app.screen.step_typing_size(bigger);
}
