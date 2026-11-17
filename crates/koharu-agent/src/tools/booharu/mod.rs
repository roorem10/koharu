//! Booharu's tools, one file per feature group, registered at the end of the
//! shared toolset, so the in-app agent and the MCP server offer them both.
//! Each takes the `booharu_` prefix, its schema from `schema::<T>()`, and
//! accurate read-only annotations, which MCP clients go by. Declared inside
//! `tools`, they reach the open project through its private fields and
//! helpers, and the app through the host the app gives
//! `Tools::with_booharu`.

use std::sync::Arc;

use booharu_core::settings::BooharuPreferences;
use rig_agent::tool::ToolSet;
use rig_core::tool::{ToolContext, ToolExecutionError};

use super::{Tools, project};

/// The app, as Booharu's tools reach it beyond the open project's session,
/// pipeline, and renderer. The desktop app gives one; a host blocks, so a
/// tool calls it on a blocking thread.
pub trait BooharuHost: Send + Sync {
    /// Every Booharu settings section, as the file holds it now; a bad stored
    /// value is an error.
    fn preferences(&self) -> anyhow::Result<BooharuPreferences>;

    /// Saves every Booharu settings section, or none when one holds a bad
    /// value, and answers them as the file holds them now.
    fn save_preferences(
        &self,
        preferences: BooharuPreferences,
    ) -> anyhow::Result<BooharuPreferences>;
}

impl Tools {
    /// These tools with `host`, which Booharu's tools reach the app through.
    pub fn with_booharu(mut self, host: Arc<dyn BooharuHost>) -> Self {
        self.booharu = Some(host);
        self
    }
}

/// Adds Booharu's tools to `tools`: none yet.
pub(crate) fn register(_tools: &mut ToolSet) {}

/// The host of the app the call runs in. Without one, as in the `serve`
/// example, a tool that needs the app fails.
#[expect(dead_code, reason = "Booharu's first tool that needs the app calls it")]
pub(crate) fn host(context: &ToolContext) -> Result<Arc<dyn BooharuHost>, ToolExecutionError> {
    project(context)?
        .booharu
        .clone()
        .ok_or_else(|| ToolExecutionError::other("this tool needs the Booharu app"))
}
