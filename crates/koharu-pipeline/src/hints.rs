use std::collections::BTreeSet;

use koharu_scene::EntityId;

/// Per-run extras for the translation stage, filled in by the app before a run.
/// The default changes nothing, so callers that don't know about it keep today's behavior.
#[derive(Clone, Debug, Default)]
pub struct TranslationHints {
    /// Text layers the translation stage leaves untouched, such as skipped SFX.
    pub skip: BTreeSet<EntityId>,
}
