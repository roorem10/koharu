use std::{path::Path, sync::Mutex};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use super::super::{Error, project::CurrentProject};

const FILE: &str = "booharu-agent.json";
static UPDATE: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Default, Deserialize, Serialize, Type)]
pub struct AgentHistory {
    pub messages: Vec<HistoryMessage>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct HistoryMessage {
    pub id: String,
    pub role: HistoryRole,
    pub text: String,
    pub error: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HistoryRole {
    User,
    Assistant,
    Divider,
    Note,
}

pub(crate) fn load(directory: &Path) -> Result<AgentHistory> {
    match std::fs::read(directory.join(FILE)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(AgentHistory::default()),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn update<R>(
    directory: &Path,
    change: impl FnOnce(&mut AgentHistory) -> R,
) -> Result<R> {
    let _guard = UPDATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut history = load(directory)?;
    let result = change(&mut history);
    booharu_context::atomic_json(&directory.join(FILE), &history)?;
    Ok(result)
}

pub(crate) fn recent(history: &AgentHistory, count: usize) -> Vec<koharu_agent::Message> {
    let start = history
        .messages
        .iter()
        .rposition(|message| message.role == HistoryRole::Divider)
        .map_or(0, |index| index + 1);
    let messages: Vec<_> = history.messages[start..]
        .iter()
        .filter(|message| {
            !message.error
                && !message.text.is_empty()
                && matches!(message.role, HistoryRole::User | HistoryRole::Assistant)
        })
        .collect();
    let start = messages.len().saturating_sub(count);
    messages[start..]
        .iter()
        .map(|message| koharu_agent::Message {
            role: if message.role == HistoryRole::User {
                koharu_agent::Role::User
            } else {
                koharu_agent::Role::Assistant
            },
            content: message.text.clone(),
        })
        .collect()
}

#[derive(Serialize, Type)]
pub struct AgentHistoryView {
    pub messages: Vec<HistoryMessage>,
    pub running: Option<koharu_agent::RunId>,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_agent_history(
    project: State<'_, CurrentProject>,
    state: State<'_, super::super::agent::AgentState>,
) -> std::result::Result<AgentHistoryView, Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    let mut history = load(project.directory().context("project has no directory")?)?;
    if state.active_run().is_none() {
        history
            .messages
            .retain(|message| message.role != HistoryRole::Assistant || !message.text.is_empty());
    }
    Ok(AgentHistoryView {
        messages: history.messages,
        running: state.active_run(),
    })
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn new_agent_conversation(
    project: State<'_, CurrentProject>,
    state: State<'_, super::super::agent::AgentState>,
) -> std::result::Result<AgentHistory, Error> {
    state.start_conversation().await?;
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    let directory = project.directory().context("project has no directory")?;
    update(directory, |history| {
        history.messages.push(HistoryMessage {
            id: koharu_scene::EntityId::new().to_string(),
            role: HistoryRole::Divider,
            text: String::new(),
            error: false,
        })
    })?;
    Ok(load(directory)?)
}

pub(crate) fn record_event(
    directory: &Path,
    assistant: &str,
    event: &koharu_agent::Event,
) -> Result<()> {
    use koharu_agent::Event;
    match event {
        Event::TextDelta { .. }
        | Event::Completed { .. }
        | Event::Failed { .. }
        | Event::Cancelled { .. }
        | Event::ToolFinished { changed: true, .. } => {
            update(directory, |history| {
                if let Event::ToolFinished { name, output, .. } = event {
                    history.messages.push(HistoryMessage {
                        id: koharu_scene::EntityId::new().to_string(),
                        role: HistoryRole::Note,
                        text: format!("{name}: {output}"),
                        error: false,
                    });
                }
                if let Some(message) = history
                    .messages
                    .iter_mut()
                    .find(|message| message.id == assistant)
                {
                    match event {
                        Event::TextDelta { delta, .. } => message.text.push_str(delta),
                        Event::Completed { message: text, .. } => message.text = text.clone(),
                        Event::Failed { message: text, .. } => {
                            message.text = text.clone();
                            message.error = true;
                        }
                        _ => {}
                    }
                }
            })?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_keeps_old_conversations_but_only_sends_recent_messages() {
        let directory = tempfile::tempdir().unwrap();
        update(directory.path(), |history| {
            for (id, role) in [
                ("old", HistoryRole::User),
                ("divider", HistoryRole::Divider),
                ("one", HistoryRole::User),
                ("two", HistoryRole::Assistant),
            ] {
                history.messages.push(HistoryMessage {
                    id: id.into(),
                    role,
                    text: id.into(),
                    error: false,
                });
            }
        })
        .unwrap();
        let history = load(directory.path()).unwrap();
        assert_eq!(history.messages.len(), 4);
        assert_eq!(recent(&history, 20).len(), 2);
        assert_eq!(recent(&history, 1)[0].content, "two");
    }
}
