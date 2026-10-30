//! Chapter context: the brief, glossary and other per-project data in the sidecar.
//!
//! Owned by WO-18. The shared foundation only adds the translation hints hook.

use anyhow::Result;
use booharu_context::ProjectContext;
use koharu_pipeline::TranslationHints;
use koharu_scene::Snapshot;

/// Builds the translation stage's extras for one pipeline run.
pub(crate) fn translation_hints(
    context: &ProjectContext,
    snapshot: &Snapshot,
) -> Result<TranslationHints> {
    Ok(TranslationHints {
        skip: super::lettering::translation_skip(context, snapshot)?,
    })
}
