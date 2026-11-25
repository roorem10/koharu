//! A block's translation as someone types it, in the Story panel or through
//! the agent: one undo step that sets its text and, for a block's first
//! translation, gives it what a typed translation lacks in Koharu: the
//! default look a new translation gets, the translator's target language,
//! and its clean-up shown, and the marker of a translation typed by hand in
//! its record, so once cleared it stays a hand edit, as the canvas's typing
//! writes it too. Where it goes is Koharu's: the scene starts a
//! block's first translation as its source's blank, in its source's place
//! (Koharu's #1494, which replaced Booharu's own placement of it, #128), as
//! the canvas types one. A box styled before it's translated holds only a
//! placeholder (`status::placeholder`), the blank the type bar merged its
//! style into, so its first translation starts in its source's place with
//! that style laid over (`status::styled_source`), as on the canvas,
//! keeping the values set on it.

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
/// placeholder a style set before translating leaves) gets a first
/// translation, labelled [`LABEL`]: where the scene starts one, in its
/// source's place and style (Koharu's #1494), a placeholder placed there
/// anew in the style set on it ([`status::styled_source`]), as the canvas
/// types one; in `target`'s language, with the block's clean-up shown under
/// it, as the Translation stage leaves one; and with the default look of
/// `look`, as a new translation gets it: the default font, or its
/// category's while Pick fonts automatically is on, and with colour
/// detection off the default colours in place of its source's, each only
/// where the placeholder set no value of its own, as the values set by hand
/// stay. Unlike the stage's, its record keeps nothing an automatic step
/// writes, as it was typed by hand, but gets the marker of a translation
/// typed by hand (`styles::typed`, through `styles::hand_typed`), in the
/// same step, so Undo takes it back with the text and a translation cleared
/// later stays a hand edit; a sound effect kept as art loses the kept mark
/// from it, as its clean-up shows again. One
/// with a translation of its own, words, a run's emptied by hand or one
/// typed by hand and cleared since, keeps its place, lettering, and
/// language, and the step is labelled as the Story panel labels a retyped
/// text.
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
    let typed = Op::SetText {
        layer: block.into(),
        text,
    };
    if !status::unwritten(layer, found) {
        return Ok(Edit {
            label: RETYPED.to_owned(),
            ops: vec![typed],
        });
    }
    // What the first translation starts as: for a block with none, the
    // source's blank `set_text` adds, and for a placeholder, that blank with
    // its style laid over, written whole, so it goes in the source's place.
    let mut base = serde_json::to_value(status::styled_source(found))?;
    if let Some(fields) = base.as_object_mut() {
        // Only `set_text` writes the words.
        fields.remove("content");
    }
    let placed = Op::UpdateLayer {
        layer: block.into(),
        patch: json!({"translation": base}),
    };
    // The look over what it starts as, and the marker of a translation typed
    // by hand, whose record drops the kept mark of a sound effect kept as
    // art, whose clean-up this shows.
    let looked = styles::typed(
        project,
        block,
        &placed,
        &look.lettering,
        look.font.as_deref(),
    )?;
    let mut ops = Vec::with_capacity(looked.len() + 3);
    if found.translation.is_some() {
        ops.push(placed);
    }
    ops.push(typed);
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
    fn a_typed_translation_starts_in_its_sources_place_in_the_target_language() -> Result<()> {
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
        // Where the scene starts a first translation (Koharu's #1494): its
        // source's blank, in its source's place, frame and style.
        let project = session.project();
        let Content::Block(found) = &project.layer(block)?.content else {
            bail!("{block} isn't a block");
        };
        assert_eq!(
            Text {
                content: typed.content.clone(),
                language: typed.language.clone(),
                ..found.source.blank()
            },
            typed
        );
        let bounds = typed.bounds().context("a frame, not point text")?;
        assert_eq!(
            (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
            (300.0, 20.0, 340.0, 140.0)
        );
        // As the Translation stage leaves one: in the target language, over
        // the clean-up shown, but with no record of a model making it, only
        // the marker of a translation typed by hand.
        assert_eq!(typed.language, english);
        let project = session.project();
        let layer = project.layer(block)?;
        let Content::Block(found) = &layer.content else {
            bail!("{block} isn't a block");
        };
        assert!(found.cleanup.visible);
        assert_eq!(
            json!(layer.attributes),
            json!({"booharu": {"auto": {"typed": true}}})
        );

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

        // Typed by hand: only the marker, so the text is the user's.
        let project = session.project();
        for block in [on, off] {
            let layer = project.layer(block)?;
            assert_eq!(attrs::get::<Auto>(layer), Some(marker()));
            assert!(edited(layer).text);
        }
        Ok(())
    }

    /// The record a first translation typed by hand leaves on a block whose
    /// record held nothing else: the marker alone.
    fn marker() -> Auto {
        Auto {
            typed: true,
            ..Auto::default()
        }
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
    fn undo_takes_the_marker_back_with_the_first_translation() -> Result<()> {
        let (_dir, mut session, block) = found("text")?;
        let english: LanguageIdentifier = "en".parse()?;
        let steps = session.history().steps.len();
        let typed = edit(
            &session.project(),
            block,
            "Why?".to_owned(),
            &english,
            &Snapshot::default(),
        )?;
        session.apply(typed)?;
        // The text and the marker, in one step.
        assert_eq!(session.history().steps.len(), steps + 1);
        assert_eq!(
            attrs::get::<Auto>(session.project().layer(block)?),
            Some(marker())
        );
        session.undo()?.context("a step")?;
        let project = session.project();
        let layer = project.layer(block)?;
        let Content::Block(found) = &layer.content else {
            bail!("{block} isn't a block");
        };
        assert!(found.translation.is_none());
        assert_eq!(attrs::get::<Auto>(layer), None);
        Ok(())
    }

    #[test]
    fn a_translation_typed_into_a_box_styled_before_is_placed_in_the_style_set_by_hand()
    -> Result<()> {
        const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
        const BLACK: Rgba = Rgba([0.0, 0.0, 0.0, 1.0]);
        let (_dir, mut session, block) = found("text")?;
        // A font and a fill picked on the type bar before it's translated
        // leave a placeholder, its source's blank in that style, which the
        // typing fills.
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
        // In the block's place, its source's, in the target language, over
        // its clean-up.
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
        // Typed by hand: only the marker, so the text and its values are
        // the user's.
        let project = session.project();
        let layer = project.layer(block)?;
        let Content::Block(found) = &layer.content else {
            bail!("{block} isn't a block");
        };
        assert!(found.cleanup.visible);
        assert_eq!(attrs::get::<Auto>(layer), Some(marker()));
        let hand = edited(layer);
        assert_eq!((hand.text, hand.style), (true, true));
        Ok(())
    }

    #[test]
    fn a_placeholder_made_before_koharus_fix_at_the_pages_corner_goes_in_its_sources_place()
    -> Result<()> {
        let (_dir, mut session, block) = found("text")?;
        // Before Koharu's #1494, the type bar's style merged into empty
        // point text at the page's corner.
        apply(
            &mut session,
            vec![op(
                json!({"op": "update_layer", "layer": block, "patch": {"translation": {
                    "frame": "point",
                    "transform": [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                    "direction": "horizontal",
                    "style": {"chars": {"family": FAMILY, "fill": FILL}},
                }}}),
            )?],
        )?;
        let corner = translation(&session, block)?;
        assert!(corner.bounds().is_none(), "{corner:?}");
        let english: LanguageIdentifier = "en".parse()?;
        let typed = edit(
            &session.project(),
            block,
            "Why?".to_owned(),
            &english,
            &Snapshot::default(),
        )?;
        assert_eq!(typed.label, LABEL);
        session.apply(typed)?;
        // In the block's place, not at the corner, in the style set on it.
        let typed = translation(&session, block)?;
        let bounds = typed.bounds().context("a frame, not point text")?;
        assert_eq!(
            (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
            (300.0, 20.0, 340.0, 140.0)
        );
        assert_eq!(
            (typed.style.chars.family.as_deref(), typed.style.chars.fill),
            (Some(FAMILY), Some(FILL))
        );
        // The rest of its look is its source's, lettered at size 32.
        assert_eq!(typed.style.chars.size, Some(32.0));
        Ok(())
    }

    #[test]
    fn a_translation_typed_by_hand_and_cleared_is_retyped_in_its_own_place() -> Result<()> {
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
        // Moved by hand, then cleared: blank, as Koharu counts it as none
        // (#1500), but typed by hand, as its record's marker says, so a
        // translation of its own left blank on purpose, not a placeholder.
        let moved = json!({"translation": {"frame": {"box": {"bounds":
            {"x": 100.0, "y": 20.0, "width": 40.0, "height": 120.0}}}}});
        apply(
            &mut session,
            vec![
                Op::UpdateLayer {
                    layer: block.into(),
                    patch: moved,
                },
                Op::SetText {
                    layer: block.into(),
                    text: String::new(),
                },
            ],
        )?;
        let layer = session.project().layer(block)?.clone();
        let Content::Block(found) = &layer.content else {
            bail!("{block} isn't a block");
        };
        assert!(!status::unwritten(&layer, found));
        assert!(edited(&layer).text);
        let retyped = edit(
            &session.project(),
            block,
            "Hm".to_owned(),
            &english,
            &Snapshot::default(),
        )?;
        assert!(matches!(retyped.ops.as_slice(), [Op::SetText { .. }]));
        assert_eq!(retyped.label, "set_text");
        session.apply(retyped)?;
        let typed = translation(&session, block)?;
        assert_eq!(typed.content.text(), "Hm");
        // Where the user moved it, not its source's place.
        assert_eq!(typed.bounds().map(|bounds| bounds.x0), Some(100.0));
        Ok(())
    }

    #[test]
    fn a_blank_translation_nobody_typed_is_placed_again_as_a_first_one() -> Result<()> {
        let (_dir, mut session, block) = found("text")?;
        let english: LanguageIdentifier = "en".parse()?;
        // Typed with no marker, as Koharu's agent edit tool types one or a
        // project saved before Booharu kept it holds one, then cleared:
        // blank, and no run wrote it, so only a placeholder; its frame
        // tells nothing, as every first translation starts in its source's
        // (#1494).
        for text in ["Why?", ""] {
            apply(
                &mut session,
                vec![Op::SetText {
                    layer: block.into(),
                    text: text.to_owned(),
                }],
            )?;
        }
        let cleared = translation(&session, block)?;
        assert!(cleared.bounds().is_some());
        let retyped = edit(
            &session.project(),
            block,
            "Hm".to_owned(),
            &english,
            &Snapshot::default(),
        )?;
        assert_eq!(retyped.label, LABEL);
        session.apply(retyped)?;
        let typed = translation(&session, block)?;
        assert_eq!(typed.content.text(), "Hm");
        // Where it went the first time, its source's place, and marked now.
        assert_eq!(typed.frame, cleared.frame);
        assert_eq!(
            attrs::get::<Auto>(session.project().layer(block)?),
            Some(marker())
        );
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
        // Its clean-up shows under the translation, and the record it was
        // alone in holds the marker of a translation typed by hand in its
        // place.
        let project = session.project();
        let layer = project.layer(block)?;
        let Content::Block(found) = &layer.content else {
            bail!("{block} isn't a block");
        };
        assert!(found.cleanup.visible);
        assert_eq!(attrs::get::<Auto>(layer), Some(marker()));
        let typed = translation(&session, block)?;
        assert_eq!(typed.content.text(), "Bam");
        assert!(typed.bounds().is_some());
        assert_eq!(typed.style.chars.family.as_deref(), Some(FAMILY));
        Ok(())
    }
}
