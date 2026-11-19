//! Booharu's part of the app: its commands, which the Booharu block of
//! `commands` lists, its events, which `events::bindings` lists, what the
//! canvas's Booharu effect does, the Booharu settings pushed to the canvas,
//! the MCP server's start on Booharu's own port, Read chapter and Explain
//! this line as jobs Stop and closing the project end (`jobs`), the agent's
//! chat kept beside its project (`agent_history`), the quality check's
//! findings, its watcher and its one-page check (`qc`), and the host
//! Booharu's agent tools reach the app through, which tells the interface
//! when they write a series or the chapter's notes, or save the
//! Instructions, has the quality check look again after those writes and
//! after a settings save, and reads the chapter for them.

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
pub(crate) mod qc;
pub(crate) mod read_chapter;
pub(crate) mod runs;
pub(crate) mod settings;
pub(crate) mod styles;
pub(crate) mod translation;

use std::sync::Arc;

use anyhow::Context as _;
use booharu::context::read::ReadSummary;
use booharu_core::settings::BooharuPreferences;
use futures::future::BoxFuture;
use koharu_agent::BooharuHost;
use koharu_scene::{Change, Edit, Session};
use parking_lot::Mutex;
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

/// Builds an edit with `build` from the open project and applies it, as
/// [`apply`] does, when there is one: none when the action changes nothing,
/// which makes no undo step.
async fn apply_if(
    app: &Arc<App>,
    build: impl FnOnce(&Session) -> anyhow::Result<Option<Edit>> + Send + 'static,
) -> Result<Option<Change>> {
    let app = Arc::clone(app);
    Ok(
        tokio::task::spawn_blocking(move || -> anyhow::Result<Option<Change>> {
            let session = app.session()?;
            let mut session = session.lock();
            let Some(edit) = build(&session)? else {
                return Ok(None);
            };
            tracing::info!(label = %edit.label, ops = edit.ops.len(), "edit");
            Ok(Some(session.apply(edit)?))
        })
        .await??,
    )
}

/// The app as Booharu's agent tools reach it, the in-app agent's and the MCP
/// server's alike, through what the commands run: the window, for the
/// canvas's settings after a save, and through its handle the app, for the
/// run groups `booharu_run` carries on with and the chapter
/// `booharu_read_chapter` reads.
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
        // Any section may change what the quality check finds, as
        // `[booharu-lettering]` regroups the odd font check's blocks.
        qc::touch_all();
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
        qc::touch_all();
    }

    fn instructions_changed(&self) {
        if let Some(handle) = self.0.handle() {
            events::INSTRUCTIONS_CHANGED.tell(&handle, ());
        }
    }

    /// A read by hand, as the Context tab's (`read_chapter`), of the tools'
    /// project while it's the one open: a Read chapter job, which a drop of
    /// the tool's call stops, as Stop does.
    fn read_chapter(
        &self,
        session: Arc<Mutex<Session>>,
        model: Option<String>,
    ) -> BoxFuture<'static, anyhow::Result<ReadSummary>> {
        let app = self
            .0
            .handle()
            .and_then(|handle| Some(Arc::clone(handle.try_state::<Arc<App>>()?.inner())));
        Box::pin(async move {
            let app = app.context("Read chapter can't start before the app is up")?;
            let open = app
                .project
                .lock()
                .as_ref()
                .is_some_and(|open| Arc::ptr_eq(&open.session, &session));
            anyhow::ensure!(open, "the project this call was made on is no longer open");
            read_chapter::read_by_hand(app, session, model).await
        })
    }
}
