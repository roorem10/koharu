//! Booharu's tools, one file per feature group, registered at the end of the
//! shared toolset, so the in-app agent and the MCP server offer them both.
//! Each takes the `booharu_` prefix, its schema from `schema::<T>()`, and
//! accurate read-only annotations, which MCP clients go by. Declared inside
//! `tools`, they reach the open project through its private fields and
//! helpers, and the app through the host the app gives
//! `Tools::with_booharu`.
//!
//! | Tool | Does | Only reads |
//! |---|---|---|
//! | `booharu_get_project_overview` | each page's size and blocks, how many are translated, and the Booharu attributes on its layers; the translator's settings, the typesetting defaults, the font families, and Booharu's settings | yes |
//! | `booharu_get_settings` | every Booharu settings section, by its table name | yes |
//! | `booharu_set_settings` | changes keys of one section, saved whole or not at all; an outside app can't change `booharu-mcp` | no |

mod project;
mod settings;

use std::sync::Arc;

use booharu_core::settings::BooharuPreferences;
use rig_agent::tool::ToolSet;

use super::Tools;

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

/// Marks a call an outside app made over MCP: the MCP server puts it in the
/// context of each call it runs, where only Booharu's own code puts anything,
/// so an outside app can neither forge nor drop it. A tool refuses through it
/// what only the user decides, such as how outside apps reach Booharu.
pub(crate) struct OverMcp;

/// Adds Booharu's tools to `tools`.
pub(crate) fn register(tools: &mut ToolSet) {
    tools.add_tool(project::GetProjectOverview);
    tools.add_tool(settings::GetSettings);
    tools.add_tool(settings::SetSettings);
}
