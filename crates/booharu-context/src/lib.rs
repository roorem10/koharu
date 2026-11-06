//! Booharu's per-project context sidecar and the lettering category of each line.
//!
//! The sidecar is a JSON file beside the project's state files, so none of this
//! changes the project save format. A deleted text layer only leaves a stale
//! entry behind, which [`ProjectContext::retain_layers`] drops.

mod chapter;
mod lettering;
mod series;
mod series_sync;
mod sidecar;

pub use lettering::LetteringCategory;
pub use sidecar::{
    CONTEXT_FILE, CONTEXT_VERSION, LabelSource, LineCategory, LineContext, ProjectContext,
};

pub use chapter::{
    ChapterBrief, ChapterCharacter, ChapterReading, Character, Flag, GlossaryEntry,
    InstructionTerm, ReadLine, SuggestedTerm, TermKind, instruction_terms,
};
pub use series::{Series, SeriesStore, atomic_json};
pub use series_sync::{
    TermConflict, TermConflictChoice, TermSyncResult, UsedTranslation,
    merge_chapter_characters, resolve_term_conflict, save_chapter_terms_to_series,
    term_conflicts,
};
