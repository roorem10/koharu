//! A text whose words carry effects of their own, which one type layer's
//! effects can't hold (`word_effects`), drawn into pixels over a copy to
//! edit. Bottom to top: the copy, a hidden type layer of its words in plain
//! styles (`plain`), so with no effects, whose pixels are its letters
//! alone, as every type layer's are; then the drawing, its borders and
//! gradients and all, as a pixel layer with no type data and no effects.
//! Both carry the layer's name, opacity, blend mode, lock, and mask, and
//! the drawing its visibility. Photoshop lists the copy directly under its
//! pixels, told apart from them by its type layer's icon. The export notes
//! the text, for its notices.

use koharu_renderer::{Drawn, ResolvedText};
use koharu_scene::{LayerId, Text};
use photocraft_psd::{MaskSpec, TaggedBlock};

use super::plain;
use crate::{Export, Leaf, Result, blocks};

/// Pushes `text`, laid out as `resolved` and drawn as `drawn`, as a hidden
/// copy to edit under a pixel layer of the drawing, both named and shown as
/// `leaf` says, but the copy hidden, and each with the `blocks` its layer
/// gathered, such as its lock, and its `mask`; and notes the text by
/// `layer`, the id its notice names: a translation's block, or a free
/// text's own.
#[allow(clippy::too_many_arguments)] // What `Export::leaf` takes, and the text.
pub(crate) fn pixels(
    export: &mut Export<'_>,
    drawn: Drawn<'_>,
    leaf: Leaf<'_>,
    blocks: Vec<TaggedBlock>,
    text: &Text,
    resolved: &ResolvedText,
    mask: Option<MaskSpec>,
    layer: LayerId,
) -> Result<()> {
    let Fallback {
        copy,
        drawing,
        noted,
    } = fallback(&leaf, blocks, text, resolved, layer, export.texts);
    export.texts += 1;
    let hidden = Leaf {
        visible: false,
        ..leaf
    };
    export.leaf(Some(drawn), hidden, copy, mask.clone())?;
    export.leaf(Some(drawn), leaf, drawing, mask)?;
    export.noted.push(noted);
    Ok(())
}

/// The blocks of the two layers `pixels` pushes, and the text as it's
/// noted.
#[derive(Debug)]
struct Fallback {
    /// The copy's blocks: those its layer gathered, then a type layer of its
    /// words in plain styles, which carries no effects.
    copy: Vec<TaggedBlock>,
    /// The drawing's blocks: those its layer gathered alone, so no type data
    /// and no effects.
    drawing: Vec<TaggedBlock>,
    /// The text as its notice names it: by its layer, and the name its
    /// layers carry.
    noted: (LayerId, String),
}

/// What `pixels` pushes for `text`, laid out as `resolved`, given the
/// `blocks` its layer gathered: the copy's type layer is numbered `index`,
/// and the text is noted by `layer` and the name `leaf` gives its layers.
fn fallback(
    leaf: &Leaf<'_>,
    blocks: Vec<TaggedBlock>,
    text: &Text,
    resolved: &ResolvedText,
    layer: LayerId,
    index: i32,
) -> Fallback {
    let mut copy = blocks.clone();
    copy.extend(blocks::type_layer(text, &plain(resolved), index));
    Fallback {
        copy,
        drawing: blocks,
        noted: (layer, leaf.name.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Blend, Color, Gradient, GradientKind, GradientStop, Rgba, Uuid};
    use photocraft_psd::{VersionedDescriptor, descriptor::Value};

    use super::*;
    use crate::{
        LOCK_ALL,
        booharu::{glyphs_only, plain_run, resolved, word_effects},
    };

    /// "Hello brave world", whose words differ in their effects: the first
    /// plain, the second with a border and a second border of its own, and
    /// the third with a gradient of its own, its first stop red as its fill.
    fn worded() -> (Text, ResolvedText) {
        let text = Text {
            content: "Hello brave world".into(),
            ..Text::default()
        };
        let mut brave = plain_run(6..12);
        brave.stroke = Some((Color::new([1.0, 1.0, 1.0, 1.0]), 3.0));
        brave.outer_stroke = Some((Color::new([0.0, 0.0, 0.0, 1.0]), 2.0));
        let mut world = plain_run(12..17);
        world.fill = Color::new([1.0, 0.0, 0.0, 1.0]);
        world.fill_gradient = Some(Gradient {
            kind: GradientKind::Linear,
            angle: 90.0,
            stops: [[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]]
                .into_iter()
                .enumerate()
                .map(|(at, color)| GradientStop {
                    offset: at as f64,
                    color: Rgba(color),
                })
                .collect(),
        });
        world.fill_extent = Some(kurbo::Rect::new(130.0, 4.0, 190.0, 30.0));
        let resolved = resolved(vec![plain_run(0..6), brave, world]);
        assert!(word_effects(&resolved));
        (text, resolved)
    }

    fn leaf(name: &str) -> Leaf<'_> {
        Leaf {
            name,
            visible: true,
            opacity: 128,
            blend: Blend::Multiply,
        }
    }

    fn keys(blocks: &[TaggedBlock]) -> Vec<String> {
        blocks
            .iter()
            .map(|block| String::from_utf8_lossy(&block.key).into_owned())
            .collect()
    }

    #[test]
    fn the_copy_is_a_plain_type_layer_and_the_drawing_holds_neither_type_nor_effects() {
        let (text, resolved) = worded();
        let lock = TaggedBlock::protection(LOCK_ALL);
        let layer = LayerId(Uuid::from_u128(1));
        let Fallback { copy, drawing, .. } = fallback(
            &leaf("Hello"),
            vec![lock.clone()],
            &text,
            &resolved,
            layer,
            3,
        );
        // Both keep the lock; only the copy is a type layer, and neither
        // carries effects.
        assert_eq!(keys(&copy), ["lspf", "TySh"]);
        assert_eq!(keys(&drawing), ["lspf"]);
        assert_eq!((&copy[0], &drawing[0]), (&lock, &lock));
        // So the copy's pixels are its letters alone, and the drawing's
        // keep its borders and gradients.
        assert!(glyphs_only(&copy));
        assert!(!glyphs_only(&drawing));
        // The copy's type layer is the plain text's, the next one numbered.
        assert_eq!(
            copy[1..],
            blocks::type_layer(&text, &plain(&resolved), 3)[..]
        );
        let data = &copy[1].data;
        let (descriptor, _) =
            VersionedDescriptor::parse_prefix(&data[2 + 48 + 2..]).expect("a text descriptor");
        assert_eq!(
            descriptor.descriptor.get("TextIndex"),
            Some(&Value::Integer(3))
        );
    }

    #[test]
    fn the_text_is_noted_by_the_layer_its_notice_names_and_its_layers_name() {
        let (text, resolved) = worded();
        // A free text by its own id and name, a translation by its block's
        // id and its words.
        for (id, name) in [(5, "Sign"), (9, "Hello brave world")] {
            let layer = LayerId(Uuid::from_u128(id));
            let noted = fallback(&leaf(name), Vec::new(), &text, &resolved, layer, 0).noted;
            assert_eq!(noted, (layer, name.to_owned()));
        }
    }
}
