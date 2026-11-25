//! What Koharu's stages did to a page's blocks, as the `c2pa` records they
//! leave on what they make tell it (F10), which the run planner reads to
//! carry on and the quality check to tell work missing from work done.
//!
//! Koharu's Translation stage records translating a block, but not a block
//! whose source it was given back unchanged: that one keeps the page's own
//! lettering, with no translation and its clean-up hidden, and only
//! Booharu's hand-edit record, written in the same edit, says the stage wrote
//! no text there ([`translation_ran`]).
//!
//! A box styled before it's translated has a translation that is only a
//! placeholder ([`placeholder`]): Koharu's type bar, and Koharu's agent's
//! edit tool, write a style change on a block with no translation into its
//! translation, which the scene starts as its source's blank (Koharu's
//! #1494). It holds no words and no run wrote it, so wherever the question
//! is whether a block is translated, it counts as no translation
//! ([`unwritten`]), as Koharu counts a blank one (#1500): the box stays
//! translatable, and a run that translates it keeps the style set by hand.

use koharu_scene::{Block, CharStyle, Content, Layer, Page};
use serde_json::Value;

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

/// Whether `block`, which `layer` holds, has a translation that is only a
/// placeholder: a style set on the block before it was translated, which
/// Koharu's type bar and its agent's edit tool write into a block with no
/// translation, and the scene starts as its source's blank, in its source's
/// place (Koharu's #1494), so its frame tells nothing. Its words are blank,
/// trimmed as the record's hash trims them ([`text_hash`]), and no run wrote
/// it: its hand-edit record holds no translation and Koharu's Translation
/// stage recorded none, or the record holds the translation of no text a
/// line given back leaves, styled since. A translation a run wrote and the
/// user emptied isn't one, as its record holds the text the run wrote, so it
/// stays a hand edit. False for a block with no translation at all.
pub fn placeholder(layer: &Layer, block: &Block) -> bool {
    let Some(translation) = &block.translation else {
        return false;
    };
    let blank = translation.content.text().trim().is_empty();
    blank && by_no_run(layer)
}

/// Whether no run wrote the translation of the block `layer` holds: its
/// hand-edit record holds no translation and Koharu's Translation stage
/// recorded none, or the record holds the translation of no text a line
/// given back leaves.
fn by_no_run(layer: &Layer) -> bool {
    match Auto::of(layer).text {
        Some(hash) => hash == text_hash(""),
        None => !recorded(layer, TRANSLATION),
    }
}

/// The character values of `block`'s translation, which `layer` holds, that
/// are its own, as a new translation's look keeps them: all of them when a
/// run wrote it; when none did, as for a [`placeholder`] or a translation
/// typed by hand, which the scene starts as its source's blank (Koharu's
/// #1494), only those its source doesn't hold, so the source's look isn't
/// read as set by hand. None for a block with no translation.
pub fn own_chars(layer: &Layer, block: &Block) -> CharStyle {
    let Some(translation) = &block.translation else {
        return CharStyle::default();
    };
    let chars = &translation.style.chars;
    if !by_no_run(layer) {
        return chars.clone();
    }
    // Compared as JSON, so every value counts, one added later too.
    let (Ok(Value::Object(own)), Ok(Value::Object(source))) = (
        serde_json::to_value(chars),
        serde_json::to_value(&block.source.style.chars),
    ) else {
        return chars.clone();
    };
    let own = own
        .into_iter()
        .filter(|(key, value)| source.get(key) != Some(value))
        .collect();
    serde_json::from_value(Value::Object(own)).unwrap_or_else(|_| chars.clone())
}

/// Whether `block`, which `layer` holds, has no translation of its own: none
/// at all, or only a [`placeholder`]. Every reader asking whether a block is
/// translated asks this, so a box styled before it's translated stays
/// translatable, while the values set on it stay the user's.
pub fn unwritten(layer: &Layer, block: &Block) -> bool {
    block.translation.is_none() || placeholder(layer, block)
}

/// Whether `block`, which `layer` holds, has no translation of its own
/// ([`unwritten`]), wasn't translated, and wasn't given back unchanged,
/// which hides its clean-up.
pub fn untranslated(layer: &Layer, block: &Block) -> bool {
    unwritten(layer, block) && !recorded(layer, TRANSLATION) && block.cleanup.visible
}

/// Whether `block`, which `layer` holds, keeps the page's own lettering: no
/// translation of its own ([`unwritten`]), and its clean-up hidden, as
/// Koharu's Translation stage leaves a line it was given back unchanged, or
/// as a clean-up hidden by hand leaves one, styled since or not.
pub fn kept(layer: &Layer, block: &Block) -> bool {
    unwritten(layer, block) && !block.cleanup.visible
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
    kept(layer, block) && translation_ran(layer)
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Created, Edit, LayerId, Op, Session};
    use serde_json::{Value, json};

    use super::*;
    use crate::{attrs, sfx, styles, testing::type_bar_style};

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

    fn op(value: Value) -> Op {
        serde_json::from_value(value).expect("an op")
    }

    fn apply(session: &mut Session, ops: Vec<Op>) {
        session
            .apply(Edit {
                label: "test".to_owned(),
                ops,
            })
            .expect("the edit");
    }

    /// Writes the block `id` as Koharu's Translation stage writes a line it
    /// was given back: no translation, the clean-up hidden, and the record
    /// of no text, with no `c2pa` record of its own.
    fn give_back(session: &mut Session, id: LayerId) {
        let project = session.project();
        let layer = project.layer(id).expect("the block");
        let ops = vec![
            op(json!({"op": "update_layer", "layer": id,
                "patch": {"translation": null, "cleanup": {"visible": false}}})),
            styles::given_back(layer).expect("the record"),
        ];
        apply(session, ops);
    }

    /// Whether the block `id` has only a placeholder, and whether it has no
    /// translation of its own.
    fn read(session: &Session, id: LayerId) -> (bool, bool) {
        let project = session.project();
        let layer = project.layer(id).expect("the block");
        (
            placeholder(layer, block(layer)),
            unwritten(layer, block(layer)),
        )
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
        assert!(!kept(layer, block(layer)) && !translation_ran(layer));

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
        assert!(
            kept(layer, block(layer)) && translation_ran(layer) && given_back(layer, block(layer))
        );
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
        assert!(kept(layer, block(layer)) && !given_back(layer, block(layer)));
        assert_eq!(attrs::get::<Auto>(layer), None);
    }

    #[test]
    fn a_style_set_before_translating_leaves_only_a_placeholder() {
        let typed = |id: LayerId, text: &str| Op::SetText {
            layer: id.into(),
            text: text.to_owned(),
        };
        let read_steps = [DETECTION, OCR];

        // No translation at all is none of its own, but no placeholder.
        let (_dir, mut session, [_, id]) = page(false, &read_steps);
        assert_eq!(read(&session, id), (false, true));
        // A font and a fill picked in the type bar start one as its
        // source's blank, in its source's place (Koharu's #1494): a
        // placeholder.
        apply(&mut session, vec![type_bar_style(id)]);
        assert_eq!(read(&session, id), (true, true));
        // A move changes only its transform: still one.
        apply(
            &mut session,
            vec![op(json!({"op": "transform_layer", "layer": id,
                "transform": [[1.0, 0.0, 12.0], [0.0, 1.0, 8.0]]}))],
        );
        assert_eq!(read(&session, id), (true, true));
        // Whitespace is no words.
        apply(&mut session, vec![typed(id, " \n")]);
        assert_eq!(read(&session, id), (true, true));
        // Words typed in are a translation.
        apply(&mut session, vec![typed(id, "Why?")]);
        assert_eq!(read(&session, id), (false, false));

        // Resized into another box, it still holds no words no run wrote:
        // its frame tells nothing since Koharu's #1494.
        let (_dir, mut session, [_, id]) = page(false, &read_steps);
        let boxed = op(json!({"op": "update_layer", "layer": id,
            "patch": {"translation": {"frame": {"box": {"bounds":
                {"x": 0, "y": 0, "width": 40, "height": 20}}}}}}));
        apply(&mut session, vec![type_bar_style(id), boxed]);
        assert_eq!(read(&session, id), (true, true));

        // Whitespace typed into a block with none, unstyled, is point text
        // no run wrote too.
        let (_dir, mut session, [_, id]) = page(false, &read_steps);
        apply(&mut session, vec![typed(id, "  ")]);
        assert_eq!(read(&session, id), (true, true));

        // A run's translation emptied by hand: the record holds the text
        // the run wrote.
        let (_dir, mut session, [_, id]) = page(false, &read_steps);
        let record = Auto {
            text: Some(text_hash("Why?")),
            ..Auto::default()
        };
        apply(
            &mut session,
            vec![
                type_bar_style(id),
                attrs::set(id, &record).expect("the record"),
            ],
        );
        assert_eq!(read(&session, id), (false, false));

        // Written by Koharu's Translation stage, with no Booharu record.
        let (_dir, mut session, [_, id]) = page(false, &[DETECTION, OCR, TRANSLATION]);
        apply(&mut session, vec![type_bar_style(id)]);
        assert_eq!(read(&session, id), (false, false));

        // A line given back, styled since: its record holds no text.
        let (_dir, mut session, [_, id]) = page(false, &read_steps);
        give_back(&mut session, id);
        apply(&mut session, vec![type_bar_style(id)]);
        assert_eq!(read(&session, id), (true, true));
    }

    #[test]
    fn a_box_styled_before_its_translated_reads_as_one_with_no_translation() {
        // Its clean-up showing: still to translate.
        let (_dir, mut session, [_, id]) = page(false, &[DETECTION, OCR]);
        apply(&mut session, vec![type_bar_style(id)]);
        let project = session.project();
        let layer = project.layer(id).expect("the block");
        assert!(untranslated(layer, block(layer)));
        assert!(!kept(layer, block(layer)) && !given_back(layer, block(layer)));

        // Its clean-up hidden by hand: the page's lettering, kept by hand.
        let hidden = op(json!({"op": "update_layer", "layer": id,
            "patch": {"cleanup": {"visible": false}}}));
        apply(&mut session, vec![hidden]);
        let project = session.project();
        let layer = project.layer(id).expect("the block");
        assert!(!untranslated(layer, block(layer)));
        assert!(kept(layer, block(layer)) && !given_back(layer, block(layer)));
        assert!(sfx::hand_kept(layer, block(layer)));

        // Given back by the Translation stage, styled since: the stage's.
        let (_dir, mut session, [_, id]) = page(false, &[DETECTION, OCR]);
        give_back(&mut session, id);
        apply(&mut session, vec![type_bar_style(id)]);
        let project = session.project();
        let layer = project.layer(id).expect("the block");
        assert!(placeholder(layer, block(layer)) && !untranslated(layer, block(layer)));
        assert!(given_back(layer, block(layer)) && translation_ran(layer));
        assert!(!sfx::hand_kept(layer, block(layer)));
    }
}
