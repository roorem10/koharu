use koharu_scene::{Component, Origin};
use revision::revisioned;

/// Consumed by the app after translation resets the box's typography.
#[revisioned(revision = 1)]
#[derive(Clone, Debug, PartialEq)]
pub struct EmphasisHints {
    pub origin: Origin,
    pub words: Vec<String>,
}

impl Component for EmphasisHints {
    const KIND: &'static str = "dev.koharu.text.emphasis_hints";

    fn origin(&self) -> Option<&Origin> {
        Some(&self.origin)
    }

    fn set_origin(&mut self, origin: Origin) -> bool {
        self.origin = origin;
        true
    }
}
