//! Lettering categories: which style a line gets, and which lines translation skips.
//!
//! Owned by WO-19. The shared foundation only fixes these signatures; WO-18 and
//! the shared commands call them and never change this file.

use std::collections::BTreeSet;

use anyhow::Result;
use booharu_context::ProjectContext;
use koharu_pipeline::bubble_shape::{BubbleShapeInfo, style_for_shape};
use koharu_renderer::{TextStyle, TextStyles};
use koharu_scene::{Commit, EntityId, Snapshot};

use super::project::Project;

/// The style for one text layer in automatic styling, applied on top of the
/// default style. `None` leaves the layer on the default style alone.
pub(crate) fn pick_style<'a>(
    styles: &'a TextStyles,
    _context: &ProjectContext,
    _snapshot: &Snapshot,
    _layer: EntityId,
    info: &BubbleShapeInfo,
) -> Result<Option<&'a TextStyle>> {
    Ok(style_for_shape(styles, info.shape))
}

/// Text layers the translation stage must leave untouched in this run.
pub(crate) fn translation_skip(
    _context: &ProjectContext,
    _snapshot: &Snapshot,
) -> Result<BTreeSet<EntityId>> {
    Ok(BTreeSet::new())
}

/// Called after the category of `layers` changed in the sidecar, by hand, by
/// the chapter pass or by the agent. Returns the restyle commit, if any; the
/// caller records it and refreshes the canvas.
pub(crate) async fn labels_changed(
    _project: &mut Project,
    _layers: &[EntityId],
) -> Result<Option<Commit>> {
    Ok(None)
}
