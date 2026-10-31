//! Creates a text box the way detection does: text region, text content and text layer.

use anyhow::Result;
use koharu_scene::{
    At, Edit, EntityId, FitsTo, FlowsIn, Geometry, Inside, RecognizedFrom, Region, TextLayout,
    TextRole, Typography,
};

/// What to create. `region` is the text region's metadata (kind text), `geometry` its polygon.
pub(crate) struct BoxDraft {
    pub page: EntityId,
    pub at: At,
    pub region: Region,
    pub geometry: Geometry,
    pub role: Option<TextRole>,
    pub layout: TextLayout,
    pub typography: Option<Typography>,
    /// Also give the layer this frame; boxes without one take their frame from the region.
    pub layer_geometry: Option<Geometry>,
    /// `Some` flows the text into this bubble region; `None` fits it to its own region.
    pub bubble: Option<EntityId>,
}

#[allow(dead_code)] // region and content are for WO-07's new boxes
#[derive(Clone, Copy, Debug)]
pub(crate) struct NewTextBox {
    pub region: EntityId,
    pub content: EntityId,
    pub layer: EntityId,
}

pub(crate) fn add_detected_style_box(edit: &mut Edit, draft: BoxDraft) -> Result<NewTextBox> {
    let region = edit.add_entity(draft.page, At::End)?;
    edit.set(region, &draft.region)?;
    edit.set(region, &draft.geometry)?;
    let content = edit.add_text_content(draft.page, At::End)?;
    if let Some(role) = &draft.role {
        edit.set(content, role)?;
    }
    edit.relate::<RecognizedFrom>(content, region)?;
    let layer = edit.add_text_layer(draft.page, draft.at, content, &draft.layout)?;
    if let Some(typography) = &draft.typography {
        edit.set(layer, typography)?;
    }
    if let Some(geometry) = &draft.layer_geometry {
        edit.set(layer, geometry)?;
    }
    match draft.bubble {
        Some(bubble) => {
            edit.relate::<FlowsIn>(layer, bubble)?;
            edit.relate::<Inside>(region, bubble)?;
        }
        None => {
            edit.relate::<FitsTo>(layer, region)?;
        }
    }
    Ok(NewTextBox {
        region,
        content,
        layer,
    })
}
