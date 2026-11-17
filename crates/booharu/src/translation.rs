//! A block's translation as someone types it, in the Story panel or through
//! the agent: one undo step that places a block's first translation as the
//! Translation stage places its own, then sets its text. The scene's
//! `set_text` alone gives a block's first translation a default text, point
//! text at the page's corner (#128), which the scene's own tests hold it to.

use anyhow::{Result, bail};
use koharu_scene::{Content, Edit, LanguageIdentifier, LayerId, Op, Project};

/// The undo step's label, which the History panel translates.
pub const LABEL: &str = "booharu_set_translation";

/// The edit setting `block`'s translation in `project` to `text`. A block
/// without one gets it placed into `target` first, as the Translation stage
/// places its own: in the block's room in its balloon, or in its source's
/// frame, at the page's lettering size. One with a translation keeps its
/// place and lettering.
pub fn edit(
    project: &Project,
    block: LayerId,
    text: String,
    target: &LanguageIdentifier,
) -> Result<Edit> {
    let Content::Block(found) = &project.layer(block)?.content else {
        bail!("layer {block} isn't a block");
    };
    let mut ops = Vec::new();
    if found.translation.is_none() {
        ops.push(koharu_pipeline::placement(project, block, target)?);
    }
    ops.push(Op::SetText {
        layer: block.into(),
        text,
    });
    Ok(Edit {
        label: LABEL.to_owned(),
        ops,
    })
}

#[cfg(test)]
mod tests {
    use anyhow::Context as _;
    use koharu_scene::{Created, Session, Text};
    use serde_json::json;

    use super::*;

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
    fn a_typed_translation_lands_in_its_blocks_frame_in_one_step() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut session = Session::create(dir.path().join("test.khr"))?;
        // A box drawn by hand, with no source text, away from the corner.
        let drawn = json!({"kind": "block", "source": {
            "frame": {"box": {"bounds": {"x": 300.0, "y": 20.0, "width": 40.0, "height": 120.0}}},
        }});
        let change = session.apply(Edit {
            label: "test".to_owned(),
            ops: vec![
                op(json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}))?,
                op(json!({"op": "add_layer", "page": "$p", "index": 0, "layer": drawn}))?,
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

        // Once it has a translation, wherever the user moved it, only the
        // text changes.
        let moved = json!({"translation": {"frame": {"box": {"bounds": {"x": 100.0, "y": 20.0, "width": 40.0, "height": 120.0}}}}});
        session.apply(Edit {
            label: "test".to_owned(),
            ops: vec![Op::UpdateLayer {
                layer: block.into(),
                patch: moved,
            }],
        })?;
        let retyped = edit(&session.project(), block, "Hi".to_owned(), &english)?;
        assert!(matches!(retyped.ops.as_slice(), [Op::SetText { .. }]));
        session.apply(retyped)?;
        let typed = translation(&session, block)?;
        assert_eq!(typed.content.text(), "Hi");
        assert_eq!(typed.bounds().map(|bounds| bounds.x0), Some(100.0));
        Ok(())
    }
}
