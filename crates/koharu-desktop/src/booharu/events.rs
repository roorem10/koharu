//! What Booharu tells the interface on its own, as Koharu's events do;
//! `events::bindings` lists them for the bindings.

use booharu::{jobs::BooharuJob, levels::LevelsProgress, qc::Finding};
use koharu_pipeline::RunId;
use koharu_scene::{LayerId, PageId};
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

/// How a Read chapter job goes: told as it starts, after each chunk of
/// pages it reads, and as it ends, always ending in a state other than
/// running.
pub const JOB: Event<BooharuJob> = Event::new("booharu_job");

/// The open project's conversation with the agent changed, as when Explain
/// this line added its answer: whatever shows it reads it again.
pub const AGENT_HISTORY_CHANGED: Event<()> = Event::new("booharu_agent_history_changed");

/// What the quality check found on a page: every finding on it, hidden
/// ones included, in reading order and then in the checks' order, which
/// replace the page's earlier ones.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub struct BooharuQc {
    pub page: PageId,
    pub findings: Vec<Finding>,
}

/// A page's findings, each time a check of it stores them: as the page
/// changes, and at once after Mark OK or Dismiss on it.
pub const QC: Event<BooharuQc> = Event::new("booharu_qc");

/// A right-click on a box on the canvas: `layer`, a text or a block of
/// `page`, which the selection holds by then, and where the pointer
/// pressed, `x` and `y` in the window's CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Type)]
pub struct BooharuBoxMenu {
    pub page: PageId,
    pub layer: LayerId,
    pub x: f64,
    pub y: f64,
}

/// A box's menu to open, as the canvas answers a right-click on one with
/// the Select, Direct Select or Text tool while nothing is typed; the
/// window gives the interface the keys, so the menu takes arrows, Enter and
/// Escape.
pub const BOX_MENU: Event<BooharuBoxMenu> = Event::new("booharu_box_menu");

/// How far a fix of page levels is: `done` of its `total` pages levelled,
/// told after each page it levels, before it changes any; the fix's command
/// answers what it did once it's done.
pub const LEVELS_PROGRESS: Event<LevelsProgress> = Event::new("booharu_levels_progress");
