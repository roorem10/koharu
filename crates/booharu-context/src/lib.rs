//! Booharu's per-project context sidecar and the lettering category of each line.
//!
//! The sidecar is a JSON file beside the project's state files, so none of this
//! changes the project save format. A deleted text layer only leaves a stale
//! entry behind, which [`ProjectContext::retain_layers`] drops.

mod lettering;
mod sidecar;

pub use lettering::LetteringCategory;
pub use sidecar::{
    CONTEXT_FILE, CONTEXT_VERSION, LabelSource, LineCategory, LineContext, ProjectContext,
};
