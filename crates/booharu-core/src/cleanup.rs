//! Where clean-up lands on a page, and the scan a page kept as it was
//! imported (D17, D37): the rules the Inpainting stage's write, Detect again,
//! page levels and the Gallery's chapter reading share.
//!
//! - [`under_lettering`] is where a Remove stroke's or a Generative Fill's
//!   result goes: under every layer that letters the page ([`lettering`]),
//!   so a translation is never covered by a clean-up made after it, and
//!   above the scan, the Original scan and the results before it, so another
//!   stroke builds on them. Draw keeps Koharu's place, the top of the page.
//! - [`detect_start`] is where Detect again starts adding its new layers,
//!   so they go above the Original scan and the results left on the page,
//!   rather than under them, as Koharu adds them at the page's start.
//! - [`original_scan`] finds the hidden, locked image layer that keeps a
//!   page's scan as it was before page levels changed it, by its attribute,
//!   never by its index, which Detect and the user move.

use koharu_scene::{Content, Hash, Layer, LayerId, Page};

use crate::{
    attrs::{self, OriginalScan},
    styles,
};

/// Whether `layer` letters the page: a text layer, a block, or a balloon, or
/// a layer holding one at any depth, as a panel or a group may. A block
/// draws its clean-up and its translation together, so nothing can go
/// between them, and a balloon holds only blocks and texts.
#[must_use]
pub fn lettering(layer: &Layer) -> bool {
    matches!(
        layer.content,
        Content::Text(_) | Content::Block(_) | Content::Balloon(_)
    ) || layer.children.iter().any(lettering)
}

/// The index among `page`'s top-level layers, in the scene's order, where an
/// area's result goes under all lettering (D17): that of the first top-level
/// layer that letters the page ([`lettering`]), so the result is drawn
/// before it and every layer after it, or the page's layer count, its top,
/// when none does. Whether a layer shows or is locked doesn't count: a
/// hidden translation letters the page again once it's shown, and a lock
/// keeps a layer as it is, never what goes under it.
#[must_use]
pub fn under_lettering(page: &Page) -> usize {
    page.layers
        .iter()
        .position(lettering)
        .unwrap_or(page.layers.len())
}

/// The index from which Detect again numbers its new top-level layers among
/// `remaining`, a page's top-level layers in the scene's order as they stand
/// once its removals are made: just after the last of them that is the
/// Original scan or an inpainted raster ([`kept_under`]) and comes before
/// the first that letters the page ([`lettering`]), or 0 when none does,
/// which is where Koharu adds them. So the new boxes go above the scan kept
/// and the results under the lettering, as a result made after them would,
/// and a result above lettering that stays, as one made before stage 14
/// sits, stays above them.
#[must_use]
pub fn detect_start(remaining: &[&Layer]) -> usize {
    let first = remaining
        .iter()
        .position(|layer| lettering(layer))
        .unwrap_or(remaining.len());
    remaining[..first]
        .iter()
        .rposition(|layer| kept_under(layer))
        .map_or(0, |at| at + 1)
}

/// Whether `layer` is one Detect again's new boxes go above: the Original
/// scan ([`is_original_scan`]), or a raster the Inpainting stage made, a
/// Remove stroke's, a Generative Fill's, or another area's result, which
/// records `c2pa.inpainting`.
fn kept_under(layer: &Layer) -> bool {
    is_original_scan(layer)
        || (matches!(layer.content, Content::Raster(_)) && styles::recorded(layer, "inpainting"))
}

/// Whether `layer` keeps a page's scan as it was before page levels changed
/// it: an image layer whose `booharu.original_scan` is true.
#[must_use]
pub fn is_original_scan(layer: &Layer) -> bool {
    matches!(layer.content, Content::Image(_)) && attrs::get::<OriginalScan>(layer).is_some()
}

/// `page`'s Original scan, the layer that keeps its scan as it was before
/// page levels first changed it, and that scan's file: the first layer at
/// any depth, in the order `Project::layers_of` lists them, a layer before
/// its children, that is one ([`is_original_scan`]). It's found by its
/// attribute, never by its index, as Detect adds its layers under it and
/// the user may move it.
#[must_use]
pub fn original_scan(page: &Page) -> Option<(LayerId, Hash)> {
    fn first(layers: &[Layer]) -> Option<(LayerId, Hash)> {
        layers.iter().find_map(|layer| match &layer.content {
            Content::Image(image) if is_original_scan(layer) => Some((layer.id, image.file)),
            _ => first(&layer.children),
        })
    }
    first(&page.layers)
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, io::Cursor};

    use image::{ImageFormat, Rgb, RgbImage};
    use koharu_scene::{Created, Edit, Op, PageId, Session};
    use serde_json::{Value, json};

    use super::*;

    /// A page of 8 by 8 pixels, `$p`, with a white image added, `file`,
    /// which image layers may show, and the layers the ops given add on it,
    /// by their labels.
    struct Fixture {
        _dir: tempfile::TempDir,
        session: Session,
        page: PageId,
        file: Hash,
        ids: BTreeMap<String, LayerId>,
    }

    impl Fixture {
        fn new(ops: impl FnOnce(Hash) -> Vec<Value>) -> Self {
            let dir = tempfile::tempdir().expect("a folder");
            let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
            let mut png = Vec::new();
            RgbImage::from_pixel(8, 8, Rgb([255; 3]))
                .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
                .expect("a PNG");
            let file = Hash::of(&png);
            let mut edit = vec![
                Op::AddFile { bytes: png },
                serde_json::from_value(json!({"op": "add_page", "id": "$p", "index": 0,
                    "size": {"width": 8, "height": 8}}))
                .expect("an op"),
            ];
            edit.extend(
                ops(file)
                    .into_iter()
                    .map(|op| serde_json::from_value::<Op>(op).expect("an op")),
            );
            let change = session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops: edit,
                })
                .expect("the page");
            let mut ids = BTreeMap::new();
            let mut page = None;
            for created in change.created {
                match created {
                    Created::Layer {
                        id,
                        label: Some(label),
                    } => {
                        ids.insert(label, id);
                    }
                    Created::Page { id, .. } => page = Some(id),
                    Created::Layer { .. } => {}
                }
            }
            Self {
                _dir: dir,
                session,
                page: page.expect("a page"),
                file,
                ids,
            }
        }

        fn page(&self) -> Page {
            self.session
                .project()
                .page(self.page)
                .expect("the page")
                .clone()
        }

        fn id(&self, label: &str) -> LayerId {
            self.ids[label]
        }

        /// Patches the layer `$label`, as hiding or locking it does.
        fn patch(&mut self, label: &str, patch: Value) {
            let op = json!({"op": "update_layer", "layer": self.id(label), "patch": patch});
            self.session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops: vec![serde_json::from_value(op).expect("an op")],
                })
                .expect("patched");
        }

        /// Each of `labels`' layers, as the page holds it now.
        fn layers(&self, labels: &[&str]) -> Vec<Layer> {
            let project = self.session.project();
            labels
                .iter()
                .map(|label| project.layer(self.id(label)).expect("a layer").clone())
                .collect()
        }
    }

    /// The op adding `layer` as `$label` at `index` among `parent`'s layers,
    /// or the page's.
    fn add(label: &str, parent: Option<&str>, index: usize, layer: Value) -> Value {
        json!({"op": "add_layer", "id": format!("${label}"), "page": "$p",
            "parent": parent.map(|parent| format!("${parent}")), "index": index, "layer": layer})
    }

    /// A raster: a Draw stroke's, or, `inpainted`, a Remove stroke's or a
    /// Generative Fill's result, as the Inpainting stage records it.
    fn raster(inpainted: bool) -> Value {
        match inpainted {
            true => json!({"kind": "raster", "attributes": {"c2pa": {
                "inpainting": {"action": "c2pa.created"}}}}),
            false => json!({"kind": "raster"}),
        }
    }

    fn block() -> Value {
        json!({"kind": "block", "source": {"content": "やあ"}})
    }

    /// The Original scan showing `file`, as page levels adds it: hidden,
    /// locked, and marked.
    fn original(file: Hash) -> Value {
        json!({"kind": "image", "file": file, "name": "Original scan", "visible": false,
            "locked": true, "attributes": {"booharu": {"original_scan": true}}})
    }

    #[test]
    fn a_result_goes_under_the_first_layer_that_letters_the_page() {
        // An empty page, and one of rasters only: the top, Koharu's place.
        let empty = Fixture::new(|_| Vec::new());
        assert_eq!(under_lettering(&empty.page()), 0);
        let rasters = Fixture::new(|_| {
            vec![
                add("draw", None, 0, raster(false)),
                add("result", None, 1, raster(true)),
            ]
        });
        assert_eq!(under_lettering(&rasters.page()), 2);
        // A panel holding a block letters the page through it, so a result
        // goes under the whole panel.
        let panel = Fixture::new(|_| {
            vec![
                add("result", None, 0, raster(true)),
                add(
                    "panel",
                    None,
                    1,
                    json!({"kind": "panel", "outline": "M0 0 L8 0 L8 8 L0 8 Z"}),
                ),
                add("paint", Some("panel"), 0, raster(false)),
                add("held", Some("panel"), 1, block()),
                add("draw", None, 2, raster(false)),
            ]
        });
        assert_eq!(under_lettering(&panel.page()), 1);
        // A free text at the top of the page, last in the scene's order.
        let text = Fixture::new(|_| {
            vec![
                add("result", None, 0, raster(true)),
                add("draw", None, 1, raster(false)),
                add("text", None, 2, json!({"kind": "text", "content": "Hi"})),
            ]
        });
        assert_eq!(under_lettering(&text.page()), 2);
        // A balloon letters the page, even with nothing in it yet.
        let balloon = Fixture::new(|_| {
            vec![
                add("draw", None, 0, raster(false)),
                add(
                    "balloon",
                    None,
                    1,
                    json!({"kind": "balloon", "outline": "M0 0 L8 0 L8 8 L0 8 Z"}),
                ),
            ]
        });
        assert_eq!(under_lettering(&balloon.page()), 1);
    }

    #[test]
    fn hidden_or_locked_lettering_still_counts() {
        let mut fixture = Fixture::new(|file| {
            vec![
                add("original", None, 0, original(file)),
                add("hidden", None, 1, block()),
                add("locked", None, 2, block()),
            ]
        });
        fixture.patch("hidden", json!({"visible": false}));
        fixture.patch("locked", json!({"locked": true}));
        // The hidden Original scan holds no lettering, so a result goes
        // above it, under the hidden block.
        assert_eq!(under_lettering(&fixture.page()), 1);
        fixture.patch("hidden", json!({"locked": true, "visible": true}));
        assert_eq!(under_lettering(&fixture.page()), 1);
    }

    #[test]
    fn detect_again_starts_above_the_original_scan_and_the_results_under_the_lettering() {
        let fixture = Fixture::new(|file| {
            vec![
                add("original", None, 0, original(file)),
                add("first", None, 1, raster(true)),
                add("second", None, 2, raster(true)),
                add("draw", None, 3, raster(false)),
                add("block", None, 4, block()),
                add("text", None, 5, json!({"kind": "text", "content": "Hi"})),
            ]
        });
        let start = |labels: &[&str]| {
            let layers = fixture.layers(labels);
            detect_start(&layers.iter().collect::<Vec<_>>())
        };
        // Nothing of the kind: Koharu's start.
        assert_eq!(start(&[]), 0);
        assert_eq!(start(&["draw"]), 0);
        assert_eq!(start(&["draw", "text"]), 0);
        assert_eq!(start(&["block", "first"]), 0);
        // In the scene's order, index 0 first, drawn first.
        assert_eq!(start(&["original", "first", "second"]), 3);
        assert_eq!(start(&["first", "draw", "second"]), 3);
        assert_eq!(start(&["first", "draw"]), 1);
        assert_eq!(start(&["draw", "original"]), 2);
        // A result above lettering that stays stays above the new boxes.
        assert_eq!(start(&["first", "block", "second"]), 1);
        assert_eq!(start(&["original", "text", "first"]), 1);
    }

    #[test]
    fn the_original_scan_is_found_by_its_attribute_at_any_depth() {
        // None on a page without one: an image without the attribute, a
        // raster with it, and an image with it false don't count.
        let none = Fixture::new(|file| {
            vec![
                add("image", None, 0, json!({"kind": "image", "file": file})),
                add(
                    "raster",
                    None,
                    1,
                    json!({"kind": "raster",
                    "attributes": {"booharu": {"original_scan": true}}}),
                ),
                add(
                    "false",
                    None,
                    2,
                    json!({"kind": "image", "file": file,
                    "attributes": {"booharu": {"original_scan": false}}}),
                ),
            ]
        });
        assert_eq!(original_scan(&none.page()), None);
        assert!(!is_original_scan(&none.layers(&["raster"])[0]));
        // Found wherever it is, here moved above a block by Detect, and in a
        // group by hand.
        let top = Fixture::new(|file| {
            vec![
                add("block", None, 0, block()),
                add("original", None, 1, original(file)),
            ]
        });
        assert_eq!(
            original_scan(&top.page()),
            Some((top.id("original"), top.file))
        );
        let grouped = Fixture::new(|file| {
            vec![
                add("group", None, 0, json!({"kind": "group"})),
                add("original", Some("group"), 0, original(file)),
                add("copy", None, 1, original(file)),
            ]
        });
        // The first in `layers_of`' order: the group's, before the copy
        // after the group.
        assert_eq!(
            original_scan(&grouped.page()),
            Some((grouped.id("original"), grouped.file))
        );
    }
}
