use revision::revisioned;

use crate::Component;

/// Anchors an external context edit to the project's revision and undo history.
/// The context owner applies its checked inverse; Scene owns the revision.
#[revisioned(revision = 1)]
#[derive(Clone, Debug, PartialEq)]
pub struct ContextEdit {
    pub token: String,
    pub reason: String,
}

impl Component for ContextEdit {
    const KIND: &'static str = "dev.booharu.context-edit";
}
