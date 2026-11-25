//! The project's conversation with the agent, kept beside its project in
//! `Ch.1.booharu-agent.json` (D18), so the chat comes back with the project:
//! the transcript the chat panel shows, Explain this line's answers among it,
//! and what the model remembers.
//!
//! The transcript is Booharu's own types, ordered by time. The memory is
//! rig's conversation as JSON, which this crate never reads, kept with the
//! rig version that wrote it, so a Booharu whose rig no longer reads it
//! starts the model afresh while the transcript stays. The model remembers
//! only what came after the last divider; the transcript shows everything.
//!
//! As with the chapter's side file, a missing file reads as empty, a file
//! that doesn't read is an error and is never replaced, and keys this
//! Booharu doesn't know are kept. Every write goes through
//! [`AgentFile::update`], under the chapter's lock, atomically, and only
//! while the project's file is still there, so a write that lands after the
//! project was deleted leaves nothing behind.

use std::{
    io::ErrorKind,
    path::Path,
    sync::PoisonError,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result};
use koharu_scene::LayerId;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{agent_file, chapter::UPDATE, series::write};

/// The version a new agent file is written with. A reader takes any, and
/// keeps it.
pub const AGENT_VERSION: u32 = 1;

/// The rig version Booharu saves the model's memory with, Koharu's pinned
/// rig's: since Koharu 0.95.0, 0.47.0, whose conversation keeps its
/// compaction's summary among its messages.
pub const RIG_VERSION: &str = "0.47.0";

/// What a project keeps of its conversation with the agent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentFile {
    pub version: u32,
    /// The transcript, ordered by [`at`](AgentEntry::at), earliest first.
    pub entries: Vec<AgentEntry>,
    /// What the model remembers of the talk since the last divider; none
    /// before anything was said, and after New chat.
    pub memory: Option<AgentMemory>,
    /// The keys this Booharu doesn't know, kept as they were.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl Default for AgentFile {
    fn default() -> Self {
        Self {
            version: AGENT_VERSION,
            entries: Vec::new(),
            memory: None,
            other: Map::new(),
        }
    }
}

/// The model's memory of the conversation, as rig keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentMemory {
    /// The model the conversation was had with, as Koharu's settings name
    /// it.
    pub model: String,
    /// When it was saved, in milliseconds since the epoch.
    pub saved_at: u64,
    /// The rig version that wrote `conversation`: [`RIG_VERSION`] when this
    /// Booharu saved it.
    pub rig: String,
    /// rig's conversation as rig wrote it, which only the agent reads.
    pub conversation: Value,
}

/// One entry of the transcript, as the chat panel shows it. Every entry has
/// an id, unique in its file, and the time it happened, in milliseconds
/// since the epoch, by which the transcript is ordered.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum AgentEntry {
    /// What the user sent.
    User { id: String, at: u64, text: String },
    /// The agent's answer, as the chat panel showed it once it ended.
    Assistant {
        id: String,
        at: u64,
        /// The model that answered, as Koharu's settings name it.
        model: String,
        state: AgentTurnState,
        /// Why it failed, when it did.
        error: Option<String>,
        parts: Vec<AgentPart>,
    },
    /// Where a new conversation starts: the model doesn't remember what is
    /// above it.
    Divider {
        id: String,
        at: u64,
        reason: AgentDividerReason,
    },
    /// An answer from outside the conversation, such as Explain this
    /// line's, which the model doesn't remember either.
    Note {
        id: String,
        at: u64,
        kind: AgentNoteKind,
        /// The text block the note is about.
        layer: LayerId,
        /// Its source text when it was asked.
        source: String,
        /// Its translation when it was asked.
        translation: String,
        explanation: String,
        /// Two or three other ways to translate the line.
        alternatives: Vec<String>,
        /// The model that answered, by the reference that answered.
        model: String,
    },
}

impl AgentEntry {
    /// The entry's id, unique in its file.
    pub fn id(&self) -> &str {
        match self {
            Self::User { id, .. }
            | Self::Assistant { id, .. }
            | Self::Divider { id, .. }
            | Self::Note { id, .. } => id,
        }
    }

    /// When it happened, in milliseconds since the epoch.
    pub fn at(&self) -> u64 {
        match self {
            Self::User { at, .. }
            | Self::Assistant { at, .. }
            | Self::Divider { at, .. }
            | Self::Note { at, .. } => *at,
        }
    }
}

/// How an agent's answer ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentTurnState {
    Done,
    Failed,
    /// Stopped, replaced by another run, or cut off by New chat or by the
    /// project closing.
    Cancelled,
}

/// A part of an agent's answer, as the chat panel shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentPart {
    Text {
        text: String,
    },
    /// The model's reasoning, with when it started and ended, in
    /// milliseconds since the epoch.
    Thinking {
        text: String,
        started: u64,
        ended: Option<u64>,
    },
    /// A tool the agent called, with its arguments as the model gave them.
    Tool {
        tool: String,
        arguments: Value,
        state: AgentToolState,
        /// Why it failed, when it did.
        error: Option<String>,
    },
}

/// How a tool call ended; one still running when its answer ended counts as
/// failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolState {
    Done,
    Failed,
}

/// Why a new conversation started.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentDividerReason {
    /// The user pressed New chat.
    NewChat,
    /// The saved memory couldn't be read back, as after an update whose rig
    /// reads it no more, so the model starts afresh.
    MemoryLost,
}

/// What a note holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentNoteKind {
    /// Explain this line's answer.
    Explain,
}

impl AgentFile {
    /// The conversation of the project file `project`, as its agent file
    /// holds it now; empty when it has none. A file that doesn't read
    /// fails, naming it.
    pub fn load(project: &Path) -> Result<Self> {
        let path = agent_file(project);
        let text = match fs_err::read(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error.into()),
        };
        serde_json::from_slice(&text).with_context(|| {
            format!(
                "Booharu's agent conversation file {} can't be read",
                path.display()
            )
        })
    }

    /// Runs `change` on the conversation of the project file `project` as
    /// its agent file holds it now and writes what it changed, in one read
    /// and write under the chapter's lock, atomically, with the entries put
    /// in order of time (entries of the same time keep their order, so a new
    /// one goes after the last that isn't later); a change that changes
    /// nothing writes nothing. Answers none, reading and writing nothing,
    /// once the project's file is gone, as after a delete. A file that
    /// doesn't read fails, and stays as it is.
    pub fn update<R>(project: &Path, change: impl FnOnce(&mut Self) -> R) -> Result<Option<R>> {
        let _held = UPDATE.lock().unwrap_or_else(PoisonError::into_inner);
        if !matches!(project.try_exists(), Ok(true)) {
            return Ok(None);
        }
        let mut file = Self::load(project)?;
        let before = file.clone();
        let changed = change(&mut file);
        file.entries.sort_by_key(AgentEntry::at);
        if file != before {
            write(&agent_file(project), &file)?;
        }
        Ok(Some(changed))
    }

    /// Adds `entry` after the last entry that isn't later than it.
    pub fn insert(&mut self, entry: AgentEntry) {
        let at = self.entries.partition_point(|old| old.at() <= entry.at());
        self.entries.insert(at, entry);
    }
}

/// A new entry id, unique for every entry: a time-ordered UUID.
pub fn entry_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Now, in milliseconds since the epoch, as entries are timed.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    #[test]
    fn the_memory_is_saved_with_the_rig_the_workspace_pins() {
        let manifest = include_str!("../../../../Cargo.toml");
        let pinned = format!("rig-core = {{ version = \"={RIG_VERSION}\"");
        assert!(
            manifest.contains(&pinned),
            "the workspace pins rig {RIG_VERSION}"
        );
    }

    /// A folder with the project file `Ch.1.khr` in it.
    fn project() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a folder");
        let project = dir.path().join("Ch.1.khr");
        fs_err::write(&project, "project").expect("a project file");
        (dir, project)
    }

    fn user(id: &str, at: u64) -> AgentEntry {
        AgentEntry::User {
            id: id.to_owned(),
            at,
            text: format!("message {id}"),
        }
    }

    fn divider(id: &str, at: u64) -> AgentEntry {
        AgentEntry::Divider {
            id: id.to_owned(),
            at,
            reason: AgentDividerReason::NewChat,
        }
    }

    fn ids(file: &AgentFile) -> Vec<&str> {
        file.entries.iter().map(AgentEntry::id).collect()
    }

    #[test]
    fn a_conversation_reads_back_as_it_was_written() -> Result<()> {
        let (_dir, project) = project();
        assert_eq!(AgentFile::load(&project)?, AgentFile::default());
        let layer = LayerId(uuid::Uuid::nil());
        let entries = vec![
            user("u1", 10),
            AgentEntry::Assistant {
                id: "a1".to_owned(),
                at: 20,
                model: "openai/gpt-5.4".to_owned(),
                state: AgentTurnState::Failed,
                error: Some("The model didn't answer.".to_owned()),
                parts: vec![
                    AgentPart::Thinking {
                        text: "Look at page 1.".to_owned(),
                        started: 11,
                        ended: Some(12),
                    },
                    AgentPart::Tool {
                        tool: "view_page".to_owned(),
                        arguments: json!({"page": 1}),
                        state: AgentToolState::Done,
                        error: None,
                    },
                    AgentPart::Text {
                        text: "It's Hina.".to_owned(),
                    },
                ],
            },
            AgentEntry::Note {
                id: "n1".to_owned(),
                at: 30,
                kind: AgentNoteKind::Explain,
                layer,
                source: "ヒナです。".to_owned(),
                translation: "It's Hina.".to_owned(),
                explanation: "She introduces herself.".to_owned(),
                alternatives: vec!["I'm Hina.".to_owned(), "Hina here.".to_owned()],
                model: "openai/gpt-5.4".to_owned(),
            },
            AgentEntry::Divider {
                id: "d1".to_owned(),
                at: 40,
                reason: AgentDividerReason::MemoryLost,
            },
        ];
        let memory = AgentMemory {
            model: "openai/gpt-5.4".to_owned(),
            saved_at: 25,
            rig: RIG_VERSION.to_owned(),
            conversation: json!({"messages": []}),
        };
        let written = AgentFile::update(&project, |file| {
            file.entries = entries.clone();
            file.memory = Some(memory.clone());
        })?;
        assert_eq!(written, Some(()));
        let read = AgentFile::load(&project)?;
        assert_eq!(read.entries, entries);
        assert_eq!(read.memory, Some(memory));
        assert_eq!(read.version, AGENT_VERSION);

        // The shape the interface reads and the file keeps.
        let text: Value = serde_json::from_slice(&fs_err::read(agent_file(&project))?)?;
        assert_eq!(
            text["entries"][0],
            json!({"role": "user", "id": "u1", "at": 10, "text": "message u1"})
        );
        assert_eq!(
            text["entries"][1]["parts"][1],
            json!({"type": "tool", "tool": "view_page", "arguments": {"page": 1}, "state": "done", "error": null})
        );
        assert_eq!(text["entries"][2]["kind"], "explain");
        assert_eq!(text["entries"][3]["reason"], "memory_lost");
        assert_eq!(text["memory"]["rig"], "0.47.0");

        // New chat clears the memory, written as null.
        AgentFile::update(&project, |file| file.memory = None)?;
        let text: Value = serde_json::from_slice(&fs_err::read(agent_file(&project))?)?;
        assert_eq!(text["memory"], Value::Null);
        Ok(())
    }

    #[test]
    fn keys_this_booharu_doesnt_know_are_kept() -> Result<()> {
        let (_dir, project) = project();
        fs_err::write(
            agent_file(&project),
            r#"{"version": 2, "entries": [], "memory": null, "pinned": ["u1"]}"#,
        )?;
        AgentFile::update(&project, |file| file.insert(user("u1", 5)))?;
        let text: Value = serde_json::from_slice(&fs_err::read(agent_file(&project))?)?;
        assert_eq!(text["pinned"], json!(["u1"]));
        assert_eq!(text["version"], 2, "a reader keeps the version it read");
        assert_eq!(ids(&AgentFile::load(&project)?), ["u1"]);
        Ok(())
    }

    #[test]
    fn a_broken_file_is_an_error_and_stays_as_it_is() -> Result<()> {
        let (_dir, project) = project();
        let path = agent_file(&project);
        for broken in [
            "{ not json",
            r#"{"entries": [{"role": "robot", "id": "x", "at": 1}]}"#,
        ] {
            fs_err::write(&path, broken)?;
            let error = AgentFile::load(&project).expect_err("a broken file");
            assert!(
                format!("{error:#}").contains(&path.display().to_string()),
                "{error:#}"
            );
            let refused = AgentFile::update(&project, |file| file.insert(user("u1", 1)));
            assert!(refused.is_err());
            assert_eq!(fs_err::read_to_string(&path)?, broken);
        }
        Ok(())
    }

    #[test]
    fn nothing_is_written_once_the_project_is_gone() -> Result<()> {
        let (_dir, project) = project();
        // A change that changes nothing writes nothing.
        assert_eq!(AgentFile::update(&project, |_| 1)?, Some(1));
        assert!(!agent_file(&project).exists());

        fs_err::remove_file(&project)?;
        let written = AgentFile::update(&project, |file| file.insert(user("u1", 1)))?;
        assert_eq!(written, None);
        assert!(!agent_file(&project).exists());
        Ok(())
    }

    #[test]
    fn entries_go_in_order_of_time() -> Result<()> {
        let (_dir, project) = project();
        AgentFile::update(&project, |file| {
            file.insert(user("u1", 10));
            file.insert(divider("d1", 30));
        })?;
        // A cancelled turn written late goes above the later divider; an
        // entry of the same time goes after the ones already there.
        AgentFile::update(&project, |file| {
            file.insert(user("u2", 20));
            file.insert(user("u3", 30));
        })?;
        assert_eq!(ids(&AgentFile::load(&project)?), ["u1", "u2", "d1", "u3"]);
        // Pushed out of order, they're put in order as they're written.
        AgentFile::update(&project, |file| file.entries.push(user("u0", 5)))?;
        assert_eq!(
            ids(&AgentFile::load(&project)?),
            ["u0", "u1", "u2", "d1", "u3"]
        );
        Ok(())
    }

    #[test]
    fn ids_are_unique_and_the_time_is_now() {
        assert_ne!(entry_id(), entry_id());
        let now = now_millis();
        // Some time after 2026 began.
        assert!(now > 1_767_225_600_000, "{now}");
    }
}
