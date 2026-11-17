//! Booharu's tools, one file per feature group, registered at the end of the
//! shared toolset, so the in-app agent and the MCP server offer them both.
//! Each takes the `booharu_` prefix, its schema from `schema::<T>()`, and
//! accurate read-only annotations, which MCP clients go by. Declared inside
//! `tools`, they reach the open project through its private fields and
//! helpers.

use rig_agent::tool::ToolSet;

/// Adds Booharu's tools to `tools`: none yet.
pub(crate) fn register(_tools: &mut ToolSet) {}
