//! What Koharu's stages did to a page's blocks, as the `c2pa` records they
//! leave on what they make tell it (F10), which the run planner reads to
//! carry on and the quality check to tell work missing from work done.
//!
//! Koharu's Translation stage records translating a block, but not a block
//! whose source it was given back unchanged: that one keeps the page's own
//! lettering, with no translation and its clean-up hidden, and only
//! Booharu's hand-edit record, written in the same edit, says the stage wrote
//! no text there ([`translation_ran`]).

use koharu_scene::{Block, Content, Layer, Page};

use crate::{
    attrs::Auto,
    styles::{recorded, text_hash},
};

/// The `c2pa` step Koharu's Detection stage records on every panel,
/// balloon, and block it adds.
pub const DETECTION: &str = "detection";
/// The `c2pa` step Koharu's Recognition stage records.
pub const OCR: &str = "ocr";
/// The `c2pa` step Koharu's Inpainting stage records.
pub const INPAINTING: &str = "inpainting";
/// The `c2pa` step Koharu's Translation stage records.
pub const TRANSLATION: &str = "translation";

/// `page`'s blocks in tree order, as Koharu's stages read them, each with
/// its layer and whether it or a layer it's in is locked.
pub fn blocks(page: &Page) -> Vec<(&Layer, &Block, bool)> {
    fn walk<'p>(layers: &'p [Layer], locked: bool, blocks: &mut Vec<(&'p Layer, &'p Block, bool)>) {
        for layer in layers {
            let locked = locked || layer.locked;
            if let Content::Block(block) = &layer.content {
                blocks.push((layer, block, locked));
            }
            walk(&layer.children, locked, blocks);
        }
    }
    let mut blocks = Vec::new();
    walk(&page.layers, false, &mut blocks);
    blocks
}

/// Whether `block` has no translation, wasn't translated, and wasn't given
/// back unchanged, which leaves no translation and hides its clean-up.
pub fn untranslated(layer: &Layer, block: &Block) -> bool {
    block.translation.is_none() && !recorded(layer, TRANSLATION) && block.cleanup.visible
}

/// Whether `block` keeps the page's own lettering: no translation, and its
/// clean-up hidden, as Koharu's Translation stage leaves a line it was given
/// back unchanged, or as a clean-up hidden by hand leaves one.
pub fn kept(block: &Block) -> bool {
    block.translation.is_none() && !block.cleanup.visible
}

/// Whether Koharu's Translation stage wrote the block `layer` holds: it
/// recorded translating it, or it was given its source back unchanged,
/// which Booharu's hand-edit record keeps as a translation of no text.
pub fn translation_ran(layer: &Layer) -> bool {
    recorded(layer, TRANSLATION) || Auto::of(layer).text == Some(text_hash(""))
}

/// Whether `block`, which `layer` holds, keeps the page's lettering because
/// Koharu's Translation stage was given its source back unchanged: the
/// stage wrote it ([`translation_ran`]) and it's [`kept`].
pub fn given_back(layer: &Layer, block: &Block) -> bool {
    kept(block) && translation_ran(layer)
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Created, Edit, LayerId, Op, Session};
    use serde_json::{Value, json};

    use super::*;
    use crate::{attrs, styles};

    /// A page holding a balloon, locked when `locked`, around one block,
    /// and a block of its own, with the `c2pa` records of `steps` on both.
    fn page(locked: bool, steps: &[&str]) -> (tempfile::TempDir, Session, [LayerId; 2]) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let c2pa: serde_json::Map<String, Value> = steps
            .iter()
            .map(|step| ((*step).to_owned(), json!({"action": "c2pa.created"})))
            .collect();
        let source = json!({"content": "原文", "frame": {"box": {"bounds":
            {"x": 0, "y": 0, "width": 10, "height": 10}}}});
        let ops = [
            json!({"op": "add_page", "id": "$page", "index": 0,
                "size": {"width": 64, "height": 64}}),
            json!({"op": "add_layer", "id": "$balloon", "page": "$page", "index": 0,
                "layer": {"kind": "balloon", "outline": "M0 0 L20 0 L20 20 L0 20 Z"}}),
            json!({"op": "add_layer", "id": "$held", "page": "$page", "parent": "$balloon",
                "index": 0, "layer": {"kind": "block", "source": source,
                    "attributes": {"c2pa": c2pa}}}),
            json!({"op": "add_layer", "id": "$free", "page": "$page", "index": 1,
                "layer": {"kind": "block", "source": source, "attributes": {"c2pa": c2pa}}}),
        ];
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(|op| serde_json::from_value(op).expect("an op"))
                    .collect(),
            })
            .expect("a page");
        let named = |name: &str| {
            change
                .created
                .iter()
                .find_map(|created| match created {
                    Created::Layer {
                        id,
                        label: Some(label),
                    } if label == name => Some(*id),
                    _ => None,
                })
                .expect("a layer")
        };
        if locked {
            // A locked layer takes no new children, so it's locked after.
            let lock = json!({"op": "update_layer", "layer": named("balloon"),
                "patch": {"locked": true}});
            session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops: vec![serde_json::from_value(lock).expect("an op")],
                })
                .expect("locked");
        }
        (dir, session, [named("held"), named("free")])
    }

    fn block(layer: &Layer) -> &Block {
        match &layer.content {
            Content::Block(block) => block,
            other => panic!("{other:?} isn't a block"),
        }
    }

    #[test]
    fn a_pages_blocks_come_in_tree_order_with_their_locks() {
        let (_dir, session, [held, free]) = page(true, &[]);
        let project = session.project();
        let page = project.pages().next().expect("a page");
        let found: Vec<(LayerId, bool)> = blocks(page)
            .into_iter()
            .map(|(layer, _, locked)| (layer.id, locked))
            .collect();
        assert_eq!(found, [(held, true), (free, false)]);
    }

    #[test]
    fn a_line_given_back_keeps_the_lettering_and_says_the_stage_ran() {
        let (_dir, mut session, [_, id]) = page(false, &[DETECTION, OCR]);
        let project = session.project();
        let layer = project.layer(id).expect("the block");
        assert!(untranslated(layer, block(layer)));
        assert!(!kept(block(layer)) && !translation_ran(layer));

        // As the Translation stage writes a line it was given back: no
        // translation, the clean-up hidden, and the record of no text,
        // with no `c2pa` record of its own.
        let ops = vec![
            serde_json::from_value::<Op>(json!({"op": "update_layer", "layer": id,
                "patch": {"translation": null, "cleanup": {"visible": false}}}))
            .expect("an op"),
            styles::given_back(layer).expect("the record"),
        ];
        session
            .apply(Edit {
                label: "test".to_owned(),
                ops,
            })
            .expect("given back");
        let project = session.project();
        let layer = project.layer(id).expect("the block");
        assert!(!recorded(layer, TRANSLATION));
        assert!(kept(block(layer)) && translation_ran(layer) && given_back(layer, block(layer)));
        assert!(!untranslated(layer, block(layer)));

        // A clean-up hidden by hand keeps the lettering too, but no stage
        // gave it back.
        let (_dir, mut session, [_, id]) = page(false, &[DETECTION, OCR]);
        let hidden = serde_json::from_value::<Op>(json!({"op": "update_layer", "layer": id,
            "patch": {"cleanup": {"visible": false}}}))
        .expect("an op");
        session
            .apply(Edit {
                label: "test".to_owned(),
                ops: vec![hidden],
            })
            .expect("hidden");
        let project = session.project();
        let layer = project.layer(id).expect("the block");
        assert!(kept(block(layer)) && !given_back(layer, block(layer)));
        assert_eq!(attrs::get::<Auto>(layer), None);
    }
}
