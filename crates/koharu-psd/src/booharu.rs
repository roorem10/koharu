//! Booharu's part of PSD export: the rules a text follows on its way into a
//! layer. A type layer's effects hold one border, one second border, and
//! one gradient fill for all its words (`effects`), so a text whose words
//! differ in them can't keep them as effects (`word_effects`): it's drawn
//! into a pixel layer over a hidden type layer of its words in plain
//! styles to edit (`pixels`, `plain`). A layer whose effects Photoshop
//! draws keeps its glyphs alone in its pixels (`glyphs_only`), and an
//! export tells which texts it drew into pixels (`Notice`). The Original
//! scan page levels keep is left out (`original_scan`).

use koharu_renderer::ResolvedText;
use koharu_scene::{Layer, LayerId, PageId};
use photocraft_psd::TaggedBlock;
use serde::Serialize;

pub(crate) use self::{effects::effects, pixels::pixels};

mod effects;
mod pixels;

/// Whether `text`'s words differ in their effects: its runs don't all share
/// one border, one second border, and one gradient fill spread over one
/// box, which one type layer's effects can't hold. Their fonts, sizes,
/// colours, faked bold and slant, and letter spacing may differ, as a type
/// layer's style runs hold those.
pub(crate) fn word_effects(text: &ResolvedText) -> bool {
    let Some(first) = text.runs.first() else {
        return false;
    };
    text.runs.iter().any(|run| {
        run.stroke != first.stroke
            || run.outer_stroke != first.outer_stroke
            || run.fill_gradient != first.fill_gradient
            || run.fill_extent != first.fill_extent
    })
}

/// `text` in plain styles, which a type layer holds with no effects: each
/// run without its border, its second border, and its gradient fill with
/// the box it spreads over, keeping its flat fill, which is a gradient's
/// first stop, its face and size, its faked bold and slant, its letter
/// spacing, and its range.
pub(crate) fn plain(text: &ResolvedText) -> ResolvedText {
    let mut plain = text.clone();
    for run in &mut plain.runs {
        run.stroke = None;
        run.outer_stroke = None;
        run.fill_gradient = None;
        run.fill_extent = None;
    }
    plain
}

/// Whether a layer with `blocks` keeps its glyphs alone in its pixels, with
/// no border: a type layer, as Photoshop draws its pixels again from its
/// text, and a layer with effects, as Photoshop draws those around them.
/// Borders in the pixels would show twice under an effect, and where no
/// effect draws them would leave a type layer as soon as it's edited.
pub(crate) fn glyphs_only(blocks: &[TaggedBlock]) -> bool {
    blocks
        .iter()
        .any(|block| matches!(&block.key, b"TySh" | b"lfx2" | b"lmfx"))
}

/// Whether `layer` is a page's Original scan, the scan as it was imported,
/// which page levels keep hidden under everything (D37): its
/// `booharu.original_scan` attribute is true. An export leaves it out, at
/// any depth, as nothing draws it. booharu-core reads the attribute the same
/// way, but PSD export doesn't depend on booharu-core.
pub(crate) fn original_scan(layer: &Layer) -> bool {
    layer
        .attributes
        .get("booharu")
        .and_then(|booharu| booharu.get("original_scan"))
        .is_some_and(|value| value.as_bool() == Some(true))
}

/// A text an export drew into its layer's pixels, as its words' effects
/// differ and one type layer's effects can't hold them, with a hidden type
/// layer directly under it to edit. Its keys serialize in alphabetical
/// order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Notice {
    /// Where its words stay editable: `hidden_plain_styles`, a hidden type
    /// layer directly under it, in plain styles.
    pub editable_copy: &'static str,
    /// The layer the text is: a translation's block, or a free text's own.
    pub layer: LayerId,
    /// The name its layers carry.
    pub name: String,
    /// The page it's on.
    pub page: PageId,
    /// Why: `word_effects_rasterized`, its words' effects went into pixels.
    pub reason: &'static str,
}

impl Notice {
    /// What an export of `page` tells about a text it noted, by its layer
    /// and name.
    pub(crate) fn rasterized(page: PageId, (layer, name): (LayerId, String)) -> Self {
        Self {
            editable_copy: "hidden_plain_styles",
            layer,
            name,
            page,
            reason: "word_effects_rasterized",
        }
    }
}

/// A run over `range` in black 24-pixel type with no effects.
#[cfg(test)]
pub(crate) fn plain_run(range: std::ops::Range<usize>) -> koharu_renderer::TextRun {
    koharu_renderer::TextRun {
        range,
        post_script_name: "Sans".to_owned(),
        size: 24.0,
        fill: koharu_scene::Color::new([0.0, 0.0, 0.0, 1.0]),
        fill_gradient: None,
        fill_extent: None,
        stroke: None,
        outer_stroke: None,
        faux_bold: false,
        faux_italic: false,
        tracking: 0.0,
    }
}

/// A text laid out as `runs`, on one line at the origin.
#[cfg(test)]
pub(crate) fn resolved(runs: Vec<koharu_renderer::TextRun>) -> ResolvedText {
    ResolvedText {
        runs,
        frame: kurbo::Rect::new(0.0, 0.0, 200.0, 30.0),
        scale: 1.0,
        hyphens: Vec::new(),
        overflowed: false,
        baseline: 24.0,
        align: koharu_scene::Align::Start,
        rtl: false,
        line_height: 1.2,
        uncovered: Vec::new(),
    }
}

/// The text descriptor a type layer's `TySh` block holds, read back with
/// photocraft: after the block's version, its transform, and its text
/// version.
#[cfg(test)]
pub(crate) fn type_text(block: &TaggedBlock) -> photocraft_psd::descriptor::Descriptor {
    let data = &block.data[2 + 48 + 2..];
    let (text, _) =
        photocraft_psd::VersionedDescriptor::parse_prefix(data).expect("a text descriptor");
    text.descriptor
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use koharu_renderer::{Renderer, RendererConfig, TextRun};
    use koharu_scene::{
        Color, Created, Edit, Gradient, GradientKind, GradientStop, Hash, Op, Rgba, Session, Uuid,
    };
    use kurbo::Rect;
    use photocraft_psd::{PsdBuilder, descriptor::Value};
    use serde_json::json;

    use super::*;
    use crate::{Export, Labels};

    const WHITE: Color = Color::new([1.0, 1.0, 1.0, 1.0]);
    const BLACK: Color = Color::new([0.0, 0.0, 0.0, 1.0]);

    /// A gradient from red to blue.
    fn gradient() -> Gradient {
        Gradient {
            kind: GradientKind::Linear,
            angle: 0.0,
            stops: [(0.0, [1.0, 0.0, 0.0, 1.0]), (1.0, [0.0, 0.0, 1.0, 1.0])]
                .into_iter()
                .map(|(offset, color)| GradientStop {
                    offset,
                    color: Rgba(color),
                })
                .collect(),
        }
    }

    /// Three runs of words, each changed by `change`, given its index.
    fn words(change: impl Fn(usize, &mut TextRun)) -> ResolvedText {
        let runs = (0..3)
            .map(|index| {
                let mut run = plain_run(index * 5..index * 5 + 5);
                change(index, &mut run);
                run
            })
            .collect();
        resolved(runs)
    }

    /// A border and second border, and a gradient spread over `extent`.
    fn effects(run: &mut TextRun, extent: Rect) {
        run.stroke = Some((WHITE, 4.0));
        run.outer_stroke = Some((BLACK, 2.0));
        run.fill_gradient = Some(gradient());
        run.fill_extent = Some(extent);
    }

    const TEXT: Rect = Rect::new(0.0, 2.0, 180.0, 30.0);
    const WORD: Rect = Rect::new(60.0, 2.0, 120.0, 30.0);

    #[test]
    fn words_differ_in_their_effects_only_by_border_or_gradient() {
        // One run shares its effects with itself, and runs differing in all
        // but their effects share them.
        let mut one = plain_run(0..15);
        effects(&mut one, TEXT);
        assert!(!word_effects(&resolved(vec![one])));
        assert!(!word_effects(&words(|index, run| {
            effects(run, TEXT);
            run.post_script_name = format!("Face{index}");
            run.size = 24.0 + index as f32;
            run.fill = Color::new([index as f32 / 3.0, 0.0, 0.0, 1.0]);
            run.faux_bold = index == 1;
            run.faux_italic = index == 2;
            run.tracking = index as f32 * 50.0;
        })));
        // A border or a second border of its own sets a word apart.
        assert!(word_effects(&words(|index, run| {
            effects(run, TEXT);
            if index == 1 {
                run.stroke = Some((BLACK, 4.0));
            }
        })));
        assert!(word_effects(&words(|index, run| {
            effects(run, TEXT);
            if index == 2 {
                run.outer_stroke = Some((BLACK, 3.0));
            }
        })));
        // So does a flat word in a gradient text, and a word given the
        // text's gradient as its own, which spreads it over its own ink.
        assert!(word_effects(&words(|index, run| {
            effects(run, TEXT);
            if index == 1 {
                run.fill_gradient = None;
                run.fill_extent = None;
            }
        })));
        assert!(word_effects(&words(|index, run| {
            effects(run, if index == 1 { WORD } else { TEXT });
        })));
        // Words apart given one gradient each spread it over their own.
        assert!(word_effects(&words(|index, run| {
            effects(run, TEXT + kurbo::Vec2::new(index as f64 * 60.0, 0.0));
        })));
    }

    #[test]
    fn plain_styles_take_off_the_effects_and_keep_the_rest() {
        let text = words(|index, run| {
            effects(run, if index == 1 { WORD } else { TEXT });
            run.post_script_name = format!("Face{index}");
            run.size = 20.0 + index as f32;
            // A gradient's first stop, as the renderer gives it.
            run.fill = Color::new([1.0, 0.0, 0.0, 1.0]);
            run.faux_bold = index == 1;
            run.faux_italic = index == 2;
            run.tracking = index as f32 * 12.5;
        });
        assert!(word_effects(&text));
        let plain = plain(&text);
        assert_eq!(plain.runs.len(), text.runs.len());
        for (plain, run) in plain.runs.iter().zip(&text.runs) {
            assert_eq!(
                (
                    plain.stroke,
                    plain.outer_stroke,
                    &plain.fill_gradient,
                    plain.fill_extent
                ),
                (None, None, &None, None)
            );
            assert_eq!(
                (
                    &plain.range,
                    &plain.post_script_name,
                    plain.size,
                    plain.fill,
                    plain.faux_bold,
                    plain.faux_italic,
                    plain.tracking
                ),
                (
                    &run.range,
                    &run.post_script_name,
                    run.size,
                    run.fill,
                    run.faux_bold,
                    run.faux_italic,
                    run.tracking
                )
            );
        }
        // The layout around the runs stays as it was.
        let runs = text.runs.clone();
        assert_eq!(
            ResolvedText {
                runs,
                ..plain.clone()
            },
            text
        );
        // So its words share their effects, none, and a type layer of it
        // carries no effects block.
        assert!(!word_effects(&plain));
        assert_eq!(super::effects(&koharu_scene::Text::default(), &plain), None);
    }

    #[test]
    fn a_type_layer_or_a_layer_with_effects_keeps_its_glyphs_alone() {
        let blocks = |keys: &[&[u8; 4]]| -> Vec<TaggedBlock> {
            keys.iter()
                .map(|key| TaggedBlock::new(**key, Vec::new()))
                .collect()
        };
        assert!(glyphs_only(&blocks(&[b"luni", b"TySh"])));
        assert!(glyphs_only(&blocks(&[b"lfx2"])));
        assert!(glyphs_only(&blocks(&[b"lmfx"])));
        assert!(!glyphs_only(&blocks(&[b"luni", b"vmsk", b"SoCo"])));
        assert!(!glyphs_only(&[]));
    }

    #[test]
    fn a_run_s_letter_spacing_is_its_style_run_s_tracking_to_the_nearest_whole() {
        // Thousandths of an em either way, rounded, and none where it's 0.
        let text = koharu_scene::Text {
            content: "Wide tight even".into(),
            ..koharu_scene::Text::default()
        };
        let runs = [(0..5, 12.6), (5..11, -12.6), (11..15, 0.0)]
            .into_iter()
            .map(|(range, tracking)| TextRun {
                tracking,
                ..plain_run(range)
            })
            .collect();
        let blocks = crate::blocks::type_layer(&text, &resolved(runs), 0);
        let descriptor = type_text(&blocks[0]);
        let Some(Value::RawData(data)) = descriptor.get("EngineData") else {
            panic!("no EngineData");
        };
        let data = String::from_utf8_lossy(data);
        let runs = data.split("/StyleRun").nth(1).expect("style runs");
        let runs = runs.split("/GridInfo").next().unwrap_or(runs);
        // Each run's style sheet, after the default run's.
        let tracking: Vec<Option<i32>> = runs
            .split("/StyleSheetData")
            .skip(2)
            .map(|style| {
                let value = style.split("/Tracking ").nth(1)?;
                value.split_whitespace().next()?.parse().ok()
            })
            .collect();
        assert_eq!(tracking, [Some(13), Some(-13), None]);
    }

    #[test]
    fn a_notice_writes_its_keys_in_alphabetical_order() -> serde_json::Result<()> {
        let page = PageId(Uuid::from_u128(1));
        let layer = LayerId(Uuid::from_u128(2));
        let notice = Notice::rasterized(page, (layer, "Words".to_owned()));
        assert_eq!(
            serde_json::to_string(&notice)?,
            r#"{"editable_copy":"hidden_plain_styles","layer":"00000000-0000-0000-0000-000000000002","name":"Words","page":"00000000-0000-0000-0000-000000000001","reason":"word_effects_rasterized"}"#
        );
        Ok(())
    }

    #[test]
    fn the_original_scan_is_left_out_of_the_export_at_any_depth() {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let mut scan = Vec::new();
        image::RgbImage::from_pixel(8, 8, image::Rgb([200; 3]))
            .write_to(&mut Cursor::new(&mut scan), image::ImageFormat::Png)
            .expect("a PNG");
        let original = |marked: bool| {
            json!({"kind": "image", "file": Hash::of(&scan), "name": "Original scan",
                "visible": false, "locked": true,
                "attributes": {"booharu": {"original_scan": marked}}})
        };
        // One under everything, as page levels add it, and one moved into a
        // group by hand.
        let ops = [
            json!({"op": "add_page", "id": "$page", "index": 0, "background": Hash::of(&scan)}),
            json!({"op": "add_layer", "id": "$top", "page": "$page", "index": 0,
                "layer": original(true)}),
            json!({"op": "add_layer", "id": "$group", "page": "$page", "index": 1,
                "layer": {"kind": "group", "name": "Moved"}}),
            json!({"op": "add_layer", "id": "$inner", "page": "$page", "parent": "$group",
                "index": 0, "layer": original(true)}),
            json!({"op": "add_layer", "id": "$false", "page": "$page", "index": 2,
                "layer": original(false)}),
        ];
        let mut edit = vec![Op::AddFile {
            bytes: scan.clone(),
        }];
        edit.extend(ops.map(|op| serde_json::from_value::<Op>(op).expect("an op")));
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops: edit,
            })
            .expect("a page");
        let labelled = |label: &str| {
            change.created.iter().find_map(|created| match created {
                Created::Layer {
                    id,
                    label: Some(own),
                } if own == label => Some(*id),
                _ => None,
            })
        };
        let project = session.project();
        let page = project.page(change.pages.added[0]).expect("the page");
        let layer = |label: &str| {
            project
                .layer(labelled(label).expect("a label"))
                .expect("a layer")
        };
        assert!(original_scan(layer("top")) && original_scan(layer("inner")));
        assert!(
            !original_scan(layer("false")),
            "marked false, it's no original"
        );
        assert!(!original_scan(layer("group")));

        // Drawing the frame needs no GPU, nor does what the export writes of
        // the page's layers but the image marked false, which it draws.
        let renderer = Renderer::new(RendererConfig::default());
        let frame = renderer.render(&session, page.id).expect("a frame");
        let labels = Labels {
            background: "Background".to_owned(),
            cleanup: "Clean-up".to_owned(),
            outline: "Balloon outline".to_owned(),
        };
        let mut export = Export {
            renderer: &renderer,
            frame: &frame,
            labels: &labels,
            size: (8, 8),
            builder: PsdBuilder::new(8, 8).resolution(72.0),
            folders: Vec::new(),
            texts: 0,
            noted: Vec::new(),
        };
        for layer in &page.layers[..2] {
            export.layer(layer).expect("written");
        }
        // The group is an empty folder, and neither original has a record.
        assert_eq!(export.folders.len(), 1);
        let file = export.builder.build().expect("a PSD");
        let names: Vec<String> = file.layers().iter().map(|record| record.name()).collect();
        assert_eq!(names, ["</Layer group>", "Moved"]);
    }
}
