//! Booharu's part of the app: its commands, which the Booharu block of
//! `commands` lists, its events, which `events::bindings` lists, what the
//! canvas's Booharu effect does, the Booharu settings pushed to the canvas,
//! the MCP server's start on Booharu's own port, Read chapter and Explain
//! this line as jobs Stop and closing the project end (`jobs`), and the host
//! Booharu's agent tools reach the app through, which tells the interface
//! when they write a series or the chapter's notes, or save the
//! Instructions.

pub(crate) mod agent_history;
pub(crate) mod boxes;
pub(crate) mod canvas;
pub(crate) mod context;
pub(crate) mod events;
pub(crate) mod explain;
pub(crate) mod fonts;
pub(crate) mod jobs;
pub(crate) mod lettering;
pub(crate) mod mcp;
pub(crate) mod notes;
pub(crate) mod ocr_box;
pub(crate) mod read_chapter;
pub(crate) mod runs;
pub(crate) mod settings;
pub(crate) mod styles;
pub(crate) mod translation;

use std::sync::Arc;

use booharu_core::settings::BooharuPreferences;
use koharu_agent::BooharuHost;
use koharu_scene::{Change, Edit, Session};
use tauri::Manager as _;

use crate::{app::App, commands::Result, screen::Screen};

/// Builds an edit with `build` from the open project and applies it as one
/// undo step, as `App::edit` applies one, but under the lock it was built
/// under, so nothing changes in between, and off the async runtime, as a box
/// action reads pixels.
async fn apply(
    app: &Arc<App>,
    build: impl FnOnce(&Session) -> anyhow::Result<Edit> + Send + 'static,
) -> Result<Change> {
    let app = Arc::clone(app);
    Ok(tokio::task::spawn_blocking(move || applied(&app, build)).await??)
}

/// `apply`'s work, on the thread that calls it.
fn applied(
    app: &App,
    build: impl FnOnce(&Session) -> anyhow::Result<Edit>,
) -> anyhow::Result<Change> {
    let session = app.session()?;
    let mut session = session.lock();
    let edit = build(&session)?;
    tracing::info!(label = %edit.label, ops = edit.ops.len(), "edit");
    Ok(session.apply(edit)?)
}

/// The app as Booharu's agent tools reach it, the in-app agent's and the MCP
/// server's alike, through what the commands run: the window, for the
/// canvas's settings after a save, and through its handle the app, for the
/// run groups `booharu_run` carries on with.
pub(crate) struct DesktopHost(pub(crate) Screen);

impl BooharuHost for DesktopHost {
    fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
        BooharuPreferences::get()
    }

    fn save_section(
        &self,
        name: &str,
        values: serde_json::Map<String, serde_json::Value>,
    ) -> anyhow::Result<BooharuPreferences> {
        let saved = BooharuPreferences::save_section(name, values)?;
        canvas::push(&self.0);
        Ok(saved)
    }

    fn font_families(&self) -> anyhow::Result<Vec<String>> {
        Ok(koharu_config::get::<koharu_renderer::RendererConfig>()?.font_families)
    }

    fn runs(&self) -> Option<Arc<dyn booharu::runs::RunHost>> {
        let handle = self.0.handle()?;
        let app: Arc<App> = Arc::clone(handle.try_state::<Arc<App>>()?.inner());
        Some(app)
    }

    fn context_changed(&self) {
        if let Some(handle) = self.0.handle() {
            events::CONTEXT_CHANGED.tell(&handle, ());
        }
    }

    fn instructions_changed(&self) {
        if let Some(handle) = self.0.handle() {
            events::INSTRUCTIONS_CHANGED.tell(&handle, ());
        }
    }
}
