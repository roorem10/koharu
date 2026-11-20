//! A block's translation as someone types it, in the Story panel or through
//! the agent: one undo step that sets its text and writes a block's first
//! translation as the Translation stage writes one it places, with the
//! default look a new translation gets. The scene's `set_text` alone gives
//! a block's first translation a default text, point text at the page's
//! corner in no language (#128), which the scene's own tests hold it to.
//! A box styled before it's translated holds only a placeholder
//! (`status::placeholder`), the empty point text the type bar merged its
//! style into, so its first translation is placed the same way, keeping the
//! values set on it.

use anyhow::{Result, bail};
use booharu_core::{settings::Snapshot, status, styles};
use koharu_scene::{Content, Edit, LanguageIdentifier, LayerId, Op, Project};
use serde_json::json;

/// The undo step's label for a block's first translation, which the History
/// panel translates.
pub const LABEL: &str = "booharu_set_translation";

/// Koharu's label for a text retyped in the Story panel, `set_text` as the
/// interface's `setText` sends it, which the History panel shows as an edited
/// text.
const RETYPED: &str = "set_text";

/// The edit setting `block`'s translation in `project` to `text`. A block
/// with no translation of its own ([`status::unwritten`]: none, or only a
/// placeholder a style set before translating leaves) gets it as the
/// Translation stage writes one it places: in the block's room in its
/// balloon, or in its source's frame, at the page's lettering size, in
/// `target`'s language, with the block's clean-up shown under it, labelled
/// [`LABEL`]. It gets the default look of `look`, as a new translation
/// does: the default font, or its category's while Pick fonts automatically
/// is on, and with colour detection off the default colours in place of the
/// measured ones, each only where the placeholder set no value of its own,
/// as the values set by hand stay. Unlike the stage's, it gets no hand-edit
/// record of its own: it was typed by hand. A sound effect kept as art has
/// the kept mark taken from the record it holds (`styles::typed`), as its
/// clean-up shows again. One with a translation of its own, blank or not,
/// keeps its place, lettering, and language, and the step is labelled as
/// the Story panel labels a retyped text.
pub fn edit(
    project: &Project,
    block: LayerId,
    text: String,
    target: &LanguageIdentifier,
    look: &Snapshot,
) -> Result<Edit> {
    let layer = project.layer(block)?;
    let Content::Block(found) = &layer.content else {
        bail!("layer {block} isn't a block");
    };
    let mut ops = vec![Op::SetText {
        layer: block.into(),
        text,
    }];
    if !status::unwritten(layer, found) {
        return Ok(Edit {
            label: RETYPED.to_owned(),
            ops,
        });
    }
    // The stage's ops for a translation it places (`written` in
    // koharu-pipeline's stages/translate), less its records, its c2pa one
    // naming the model that made the translation and Booharu's hand-edit
    // one: someone typed this one. `typed` still drops the kept mark of a
    // sound effect kept as art, whose clean-up this shows.
    let placed = koharu_pipeline::placement(project, block, target)?;
    let looked = styles::typed(
        project,
        block,
        &placed,
        &look.lettering,
        look.font.as_deref(),
    )?;
    ops.push(placed);
    ops.extend(looked);
    ops.push(Op::UpdateLayer {
        layer: block.into(),
        patch: json!({"translation": {"language": target}, "cleanup": {"visible": true}}),
    });
    Ok(Edit {
        label: LABEL.to_owned(),
        ops,
    })
}

#[cfg(test)]
mod tests {
    use anyhow::Context as _;
    use booharu_core::{
        attrs::{self, Auto},
        edited,
        settings::{DefaultTextStyle, LetteringSettings},
        sfx,
    };
    use koharu_scene::{Created, Rgba, Session, Text};

    use super::*;
    use crate::testing::{FAMILY, FILL, type_bar_style};

    /// The translation `block` holds in `session`.
    fn translation(session: &Session, block: LayerId) -> Result<Text> {
        match &session.project().layer(block)?.content {
            Content::Block(found) => found.translation.clone().context("a translation"),
            _ => bail!("{block} isn't a block"),
        }
    }

    fn op(op: serde_json::Value) -> Result<Op> {
        Ok(serde_json::from_value(op)?)
    }

    #[test]
    fn a_typed_translation_is_placed_as_the_translation_stage_places_one() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut session = Session::create(dir.path().join("test.khr"))?;
        // A box drawn by hand, with no source text, away from the corner,
        // its clean-up hidden.
        let drawn = json!({"kind": "block", "source": {
            "frame": {"box": {"bounds": {"x": 300.0, "y": 20.0, "width": 40.0, "height": 120.0}}},
        }});
        let change = session.apply(Edit {
            label: "test".to_owned(),
            ops: vec![
                op(json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}))?,
                op(json!({"op": "add_layer", "id": "$b", "page": "$p", "index": 0, "layer": drawn}))?,
                op(json!({"op": "update_layer", "layer": "$b", "patch": {"cleanup": {"visible": false}}}))?,
            ],
        })?;
        let block = change.created.iter().find_map(|created| match created {
            Created::Layer { id, .. } => Some(*id),
            Created::Page { .. } => None,
        });
        let block = block.context("the block")?;
        let english: LanguageIdentifier = "en".parse()?;
        let steps = session.history().steps.len();

        session.apply(edit(
            &session.project(),
            block,
            "Hello".to_owned(),
            &english,
            &Snapshot::default(),
        )?)?;

        let history = session.history();
        assert_eq!(history.steps.len(), steps + 1);
        assert_eq!(history.steps.last().map(String::as_str), Some(LABEL));
        let typed = translation(&session, block)?;
        assert_eq!(typed.content.text(), "Hello");
        let bounds = typed.bounds().context("a frame, not point text")?;
        assert_eq!(
            (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
            (300.0, 20.0, 340.0, 140.0)
        );
        // As the Translation stage leaves one: in the target language, over
        // the clean-up shown, but with no record of a model making it.
        assert_eq!(typed.language, english);
        let project = session.project();
        let layer = project.layer(block)?;
        let Content::Block(found) = &layer.content else {
            bail!("{block} isn't a block");
        };
        assert!(found.cleanup.visible);
        assert!(layer.attributes.is_empty(), "{:?}", layer.attributes);

        // Once it has a translation, wherever the user moved it, only the
        // text changes, in the language it has, and History names the step as
        // it names a text retyped in the Story panel.
        let moved = json!({"translation": {"frame": {"box": {"bounds": {"x": 100.0, "y": 20.0, "width": 40.0, "height": 120.0}}}}});
        session.apply(Edit {
            label: "test".to_owned(),
            ops: vec![Op::UpdateLayer {
                layer: block.into(),
                patch: moved,
            }],
        })?;
        let retyped = edit(
            &session.project(),
            block,
            "Hi".to_owned(),
            &english,
            &Snapshot::default(),
        )?;
        assert!(matches!(retyped.ops.as_slice(), [Op::SetText { .. }]));
        assert_eq!(retyped.label, "set_text");
        session.apply(retyped)?;
        let typed = translation(&session, block)?;
        assert_eq!(typed.content.text(), "Hi");
        assert_eq!(typed.bounds().map(|bounds| bounds.x0), Some(100.0));
        assert_eq!(typed.language, english);
        Ok(())
    }

    #[test]
    fn a_typed_translation_gets_the_default_look_and_stays_the_users() -> Result<()> {
        const BLUE: Rgba = Rgba([0.0, 0.0, 1.0, 1.0]);
        const WHITE: Rgba = Rgba([1.0; 4]);
        const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
        let dir = tempfile::tempdir()?;
        let mut session = Session::create(dir.path().join("test.khr"))?;
        // Two blocks found and read by Koharu's stages, lettered in blue
        // outlined in white.
        let found = |x: f64| {
            json!({"kind": "block", "source": {
                "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 120.0}}},
                "content": "どうして",
                "style": {"chars": {"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0}},
            }, "attributes": {"c2pa": {
                "detection": {"action": "c2pa.created"},
                "ocr": {"action": "c2pa.created"},
            }}})
        };
        let change = session.apply(Edit {
            label: "test".to_owned(),
            ops: vec![
                op(json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}))?,
                op(json!({"op": "add_layer", "id": "$on", "page": "$p", "index": 0, "layer": found(10.0)}))?,
                op(json!({"op": "add_layer", "id": "$off", "page": "$p", "index": 1, "layer": found(60.0)}))?,
            ],
        })?;
        let id = |label: &str| {
            change.created.iter().find_map(|created| match created {
                Created::Layer {
                    id,
                    label: Some(found),
                } if found == label => Some(*id),
                _ => None,
            })
        };
        let (on, off) = (id("on").context("on")?, id("off").context("off")?);
        let english: LanguageIdentifier = "en".parse()?;
        let look = |detect_colors: bool| Snapshot {
            lettering: LetteringSettings {
                default: DefaultTextStyle {
                    fill: Some(RED),
                    stroke: None,
                    stroke_width: 1.5,
                },
                detect_colors,
                ..LetteringSettings::default()
            },
            font: Some("CCWildWords".to_owned()),
            ..Snapshot::default()
        };

        // Detection on: the default font, and the measured colours stay.
        let typed = edit(
            &session.project(),
            on,
            "Why?".to_owned(),
            &english,
            &look(true),
        )?;
        session.apply(typed)?;
        let chars = translation(&session, on)?.style.chars;
        assert_eq!(chars.family.as_deref(), Some("CCWildWords"));
        assert_eq!(
            (chars.fill, chars.stroke, chars.stroke_width),
            (Some(BLUE), Some(WHITE), Some(3.0))
        );
        // Off: the default colours in their place, none for the outline.
        let typed = edit(
            &session.project(),
            off,
            "Why?".to_owned(),
            &english,
            &look(false),
        )?;
        session.apply(typed)?;
        let chars = translation(&session, off)?.style.chars;
        assert_eq!(
            (chars.family.as_deref(), chars.fill, chars.stroke),
            (Some("CCWildWords"), Some(RED), None)
        );

        // Typed by hand: no record, so the text is the user's.
        let project = session.project();
        for block in [on, off] {
            let layer = project.layer(block)?;
            assert_eq!(attrs::get::<Auto>(layer), None);
            assert!(edited(layer).text);
        }
        Ok(())
    }

    /// A project of one 400-pixel page holding a block of `role` found and
    /// read by Koharu's stages, its source's box away from the page's
    /// corner, lettered at size 32 in blue outlined in white.
    fn found(role: &str) -> Result<(tempfile::TempDir, Session, LayerId)> {
        let dir = tempfile::tempdir()?;
        let mut session = Session::create(dir.path().join("test.khr"))?;
        let block = json!({"kind": "block", "role": role, "source": {
            "frame": {"box": {"bounds": {"x": 300.0, "y": 20.0, "width": 40.0, "height": 120.0}}},
            "content": "どうして",
            "style": {"chars": {"size": 32.0, "fill": [0.0, 0.0, 1.0, 1.0],
                "stroke": [1.0, 1.0, 1.0, 1.0], "stroke_width": 3.0}},
        }, "attributes": {"c2pa": {
            "detection": {"action": "c2pa.created"},
            "ocr": {"action": "c2pa.created"},
        }}});
        let change = session.apply(Edit {
            label: "test".to_owned(),
            ops: vec![
                op(json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}))?,
                op(json!({"op": "add_layer", "id": "$b", "page": "$p", "index": 0, "layer": block}))?,
            ],
        })?;
        let block = change.created.iter().find_map(|created| match created {
            Created::Layer { id, .. } => Some(*id),
            Created::Page { .. } => None,
        });
        Ok((dir, session, block.context("the block")?))
    }

    /// Applies `ops` to `session` as one edit, as the user's.
    fn apply(session: &mut Session, ops: Vec<Op>) -> Result<()> {
        session.apply(Edit {
            label: "test".to_owned(),
            ops,
        })?;
        Ok(())
    }

    #[test]
    fn a_translation_typed_into_a_box_styled_before_is_placed_in_the_style_set_by_hand()
    -> Result<()> {
        const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
        const BLACK: Rgba = Rgba([0.0, 0.0, 0.0, 1.0]);
        let (_dir, mut session, block) = found("text")?;
        // A font and a fill picked on the type bar before it's translated
        // leave a placeholder, empty point text, which the typing places.
        apply(&mut session, vec![type_bar_style(block)])?;
        let english: LanguageIdentifier = "en".parse()?;
        let look = Snapshot {
            lettering: LetteringSettings {
                default: DefaultTextStyle {
                    fill: Some(RED),
                    stroke: Some(BLACK),
                    stroke_width: 1.5,
                },
                detect_colors: false,
                ..LetteringSettings::default()
            },
            font: Some("CCWildWords".to_owned()),
            ..Snapshot::default()
        };
        let typed = edit(
            &session.project(),
            block,
            "Why?".to_owned(),
            &english,
            &look,
        )?;
        assert_eq!(typed.label, LABEL);
        session.apply(typed)?;
        // In the block's place, not at the page's corner, in the target
        // language, over its clean-up.
        let typed = translation(&session, block)?;
        assert_eq!(typed.content.text(), "Why?");
        let bounds = typed.bounds().context("a frame, not point text")?;
        assert_eq!(
            (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
            (300.0, 20.0, 340.0, 140.0)
        );
        assert_eq!(typed.language, english);
        // The family and fill set by hand stay; the look fills the outline,
        // left unset, and no default font goes over the family.
        let chars = typed.style.chars;
        assert_eq!(
            (chars.family.as_deref(), chars.fill),
            (Some(FAMILY), Some(FILL))
        );
        assert_eq!((chars.stroke, chars.stroke_width), (Some(BLACK), Some(1.5)));
        // Typed by hand: no record, so the text and its values are the
        // user's.
        let project = session.project();
        let layer = project.layer(block)?;
        let Content::Block(found) = &layer.content else {
            bail!("{block} isn't a block");
        };
        assert!(found.cleanup.visible);
        assert_eq!(attrs::get::<Auto>(layer), None);
        let hand = edited(layer);
        assert_eq!((hand.text, hand.style), (true, true));
        Ok(())
    }

    #[test]
    fn a_blank_translation_already_placed_is_retyped_in_place() -> Result<()> {
        let (_dir, mut session, block) = found("text")?;
        let english: LanguageIdentifier = "en".parse()?;
        let typed = edit(
            &session.project(),
            block,
            "Why?".to_owned(),
            &english,
            &Snapshot::default(),
        )?;
        session.apply(typed)?;
        // Cleared by hand: blank, but placed, so no placeholder.
        apply(
            &mut session,
            vec![Op::SetText {
                layer: block.into(),
                text: String::new(),
            }],
        )?;
        let cleared = translation(&session, block)?;
        assert!(cleared.bounds().is_some());
        let retyped = edit(
            &session.project(),
            block,
            "Hm".to_owned(),
            &english,
            &Snapshot::default(),
        )?;
        assert_eq!(retyped.label, "set_text");
        assert!(matches!(retyped.ops.as_slice(), [Op::SetText { .. }]));
        session.apply(retyped)?;
        let typed = translation(&session, block)?;
        assert_eq!(typed.content.text(), "Hm");
        assert_eq!((typed.frame, typed.style), (cleared.frame, cleared.style));
        Ok(())
    }

    #[test]
    fn a_translation_typed_into_an_effect_kept_as_art_and_styled_since_shows_its_clean_up()
    -> Result<()> {
        let (_dir, mut session, block) = found("sfx")?;
        // A run kept the effect as art, then the type bar styled it.
        let kept = sfx::kept_original(session.project().layer(block)?);
        assert_eq!(kept.len(), 2);
        apply(&mut session, kept)?;
        apply(&mut session, vec![type_bar_style(block)])?;
        let english: LanguageIdentifier = "en".parse()?;
        let typed = edit(
            &session.project(),
            block,
            "Bam".to_owned(),
            &english,
            &Snapshot::default(),
        )?;
        assert_eq!(typed.label, LABEL);
        session.apply(typed)?;
        // Its clean-up shows under the translation, and the mark goes with
        // the record it was alone in.
        let project = session.project();
        let layer = project.layer(block)?;
        let Content::Block(found) = &layer.content else {
            bail!("{block} isn't a block");
        };
        assert!(found.cleanup.visible);
        assert_eq!(attrs::get::<Auto>(layer), None);
        let typed = translation(&session, block)?;
        assert_eq!(typed.content.text(), "Bam");
        assert!(typed.bounds().is_some());
        assert_eq!(typed.style.chars.family.as_deref(), Some(FAMILY));
        Ok(())
    }
}
