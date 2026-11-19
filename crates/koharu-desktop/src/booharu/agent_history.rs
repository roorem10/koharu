//! The open project's conversation with the agent as the chat panel shows
//! it (S7-k), kept beside the project in its agent file
//! (`booharu_core::context::agent`): what the user and the agent said, and
//! Explain this line's answers among it. The model's memory, which the file
//! also keeps, never goes to the interface.

use std::sync::Arc;

use booharu_core::context::agent::{AgentEntry, AgentFile};
use tauri::State;

use crate::{app::App, commands::Result};

/// Every entry of the open project's conversation with the agent, earliest
/// first, old conversations' too; never the model's memory. A file that
/// doesn't read fails, naming it.
#[tauri::command]
#[specta::specta]
pub async fn booharu_get_agent_history(app: State<'_, Arc<App>>) -> Result<Vec<AgentEntry>> {
    let path = app.session()?.lock().path().to_owned();
    Ok(
        tokio::task::spawn_blocking(move || AgentFile::load(&path).map(|file| file.entries))
            .await??,
    )
}
