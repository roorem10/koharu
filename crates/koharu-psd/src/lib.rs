//! PSD export directly from Koharu scene revisions and renderer-resolved presentation.
//!
//! The binary layout follows GIMP's PSD plug-in at the pinned revision linked from every
//! format implementation. Scene semantics remain owned by `koharu-scene`; font selection,
//! layout, raster fallback, and the merged preview remain owned by `koharu-renderer`.

mod descriptor;
mod document;
mod engine_data;
mod error;
mod export;
mod packbits;
mod writer;

pub use document::{PsdExportOptions, TextLayerMode};
pub use error::PsdExportError;
pub use export::export_page;

/// Machine-readable lines saved beside PSDs whose word paint needs a pixel layer.
/// Plain font/size/colour runs remain in the hidden editable copy.
pub fn export_notices(
    frame: &koharu_renderer::Frame,
) -> Vec<std::collections::BTreeMap<&'static str, String>> {
    frame
        .layers()
        .iter()
        .filter_map(|layer| {
            let koharu_renderer::LayerKind::Text(text) = layer.kind() else {
                return None;
            };
            text.word_effects.then(|| {
                std::collections::BTreeMap::from([
                    ("page", frame.page().to_string()),
                    ("layer", layer.entity().to_string()),
                    ("reason", "word_effects_rasterized".to_owned()),
                    ("editable_copy", "hidden_plain_styles".to_owned()),
                ])
            })
        })
        .collect()
}
