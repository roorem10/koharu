//! Booharu's part of the app: its commands, which the Booharu block of
//! `commands` lists, its events, which `events::bindings` lists, the MCP
//! server's start on Booharu's own port, and the host Booharu's agent tools
//! reach the app through.

pub(crate) mod boxes;
pub(crate) mod events;
pub(crate) mod mcp;
pub(crate) mod notes;
pub(crate) mod settings;
pub(crate) mod translation;

use std::sync::Arc;

use booharu_core::settings::BooharuPreferences;
use koharu_agent::BooharuHost;
use koharu_scene::{Change, Edit, Session};

use crate::{app::App, commands::Result};

/// Builds an edit with `build` from the open project and applies it as one
/// undo step, as `App::edit` applies one, but under the lock it was built
/// under, so nothing changes in between, and off the async runtime, as a box
/// action reads pixels.
async fn apply(
    app: &Arc<App>,
    build: impl FnOnce(&Session) -> anyhow::Result<Edit> + Send + 'static,
) -> Result<Change> {
    let app = Arc::clone(app);
    Ok(
        tokio::task::spawn_blocking(move || -> anyhow::Result<Change> {
            let session = app.session()?;
            let mut session = session.lock();
            let edit = build(&session)?;
            tracing::info!(label = %edit.label, ops = edit.ops.len(), "edit");
            Ok(session.apply(edit)?)
        })
        .await??,
    )
}

/// The app as Booharu's agent tools reach it, the in-app agent's and the MCP
/// server's alike, through what the commands run.
pub(crate) struct DesktopHost;

impl BooharuHost for DesktopHost {
    fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
        BooharuPreferences::get()
    }

    fn save_preferences(
        &self,
        preferences: BooharuPreferences,
    ) -> anyhow::Result<BooharuPreferences> {
        preferences.save()
    }
}
