//! What Booharu tells the interface on its own, as Koharu's events do;
//! `events::bindings` lists them for the bindings.

use koharu_pipeline::RunId;
use serde::Serialize;
use specta::Type;

use crate::events::Event;

/// Why a box drawn with the O tool added nothing, or what failed reading it,
/// for the interface to say in the user's language.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BooharuNotice {
    /// The box holds no ink to read.
    NothingToRead,
    /// The box would change a locked layer.
    Locked,
    /// The box couldn't be added, read, translated, or cleaned.
    Failed { message: String },
}

/// A Booharu tool's notice that it did nothing, or failed.
pub const NOTICE: Event<BooharuNotice> = Event::new("booharu_notice");

/// A run a carrying-on group queued: the run of the `index`th of the
/// `count` groups `booharu_run` planned, counted from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
pub struct BooharuRunGroup {
    pub group: u64,
    pub run: RunId,
    pub index: usize,
    pub count: usize,
}

/// Each run a carrying-on group queues, so the interface knows a group's
/// runs whoever started it.
pub const RUN_GROUP: Event<BooharuRunGroup> = Event::new("booharu_run_group");

/// A series file, or the open project's side file, was written, by a
/// command or the agent: whatever shows the series glossary or the chapter's
/// notes reads them again.
pub const CONTEXT_CHANGED: Event<()> = Event::new("booharu_context_changed");

/// The translator's Instructions were saved by Import from instructions, the
/// command's or the agent's, rather than by the Settings page: the interface
/// reads Koharu's settings again, so its next save of them keeps the
/// Instructions as they are now.
pub const INSTRUCTIONS_CHANGED: Event<()> = Event::new("booharu_instructions_changed");
