use koharu_scene::Component;
use revision::revisioned;
use serde::{Deserialize, Serialize};

/// Connects sidecar context transactions to scene patch preconditions and undo.
/// The reference data remains in the sidecar; this token only tracks its revision.
#[revisioned(revision = 1)]
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TranslationContextRevision {
    pub token: String,
}

impl Component for TranslationContextRevision {
    const KIND: &'static str = "dev.booharu.translation-context-revision";
}
