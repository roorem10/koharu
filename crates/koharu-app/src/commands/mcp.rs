use std::sync::Arc;

use tauri::State;

use super::Error;

mod server;
mod settings;

pub(crate) use settings::{McpOptions, McpState, McpStatus};

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_mcp_status(state: State<'_, Arc<McpState>>) -> Result<McpStatus, Error> {
    Ok(state.status().await?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_mcp_settings(
    options: McpOptions,
    state: State<'_, Arc<McpState>>,
) -> Result<McpStatus, Error> {
    Ok(state.update(Some(options), false).await?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn new_mcp_token(state: State<'_, Arc<McpState>>) -> Result<McpStatus, Error> {
    Ok(state.update(None, true).await?)
}
