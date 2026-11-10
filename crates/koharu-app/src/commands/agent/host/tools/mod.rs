//! The tools the Agent tab and the MCP server offer, one file per group.
//!
//! To add a tool, put its arguments, definition and handler in the group it
//! belongs to. To add a group, create its file and add its name to the list
//! at the bottom of this file; nothing else needs to change.

use super::*;

/// Declares the tool groups. Each one provides `tools()` and
/// `invoke(host, call, control)`, which returns `None` for a tool it doesn't own.
macro_rules! tool_groups {
    ($($group:ident),* $(,)?) => {
        $(mod $group;)*

        pub(super) fn all() -> Vec<Tool> {
            let mut tools = Vec::new();
            $(tools.extend($group::tools());)*
            tools
        }

        pub(super) async fn invoke(
            host: &KoharuHost,
            call: &ToolCall,
            control: &Control,
        ) -> Result<Invocation> {
            $(
                if let Some(invocation) = $group::invoke(host, call, control).await? {
                    return Ok(invocation);
                }
            )*
            bail!("unknown Booharu tool {}", call.name)
        }
    };
}

// The order here is the order tools are listed to the model.
tool_groups!(pages, text, typography, context, elements, pipeline, boxes);
