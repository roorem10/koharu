//! Typing a block's translation in the Story panel: a block without one of
//! its own, none or only the placeholder a style set before translating
//! leaves, gets its first translation where Koharu's scene starts one, in
//! its source's place (#1494), as the canvas types one, in the same undo
//! step as what Koharu's lacks: the translator's target language, its
//! clean-up shown, and the default look a new translation gets where the
//! placeholder set no value of its own, with the marker of a translation
//! typed by hand in the block's record, so once cleared it stays a hand
//! edit.

use std::sync::Arc;

use booharu_core::settings::Snapshot;
use koharu_pipeline::PipelineConfig;
use koharu_renderer::RendererConfig;
use koharu_scene::{Change, LanguageIdentifier, LayerId, Session};
use tauri::State;

use crate::{app::App, commands::Result};

/// Sets `block`'s translation to `text` as one undo step, and when the
/// block has none of its own, none or only the placeholder a style set
/// before translating leaves, whose values stay, writes it as a first
/// translation: in its source's place, in the translator's target language,
/// over its clean-up shown, with the default look, and with the marker of a
/// translation typed by hand in the same step, so once cleared it stays a
/// hand edit.
#[tauri::command]
#[specta::specta]
pub async fn booharu_set_translation(
    app: State<'_, Arc<App>>,
    block: LayerId,
    text: String,
) -> Result<Change> {
    let app = Arc::clone(&app);
    // As `App::edit` applies an edit, but under the lock the edit is made
    // under, so a first translation is written only where the edit finds
    // none.
    Ok(
        tokio::task::spawn_blocking(move || -> anyhow::Result<Change> {
            let target = koharu_config::get::<PipelineConfig>()?.translator.target;
            // A new translation's default look, as Booharu's settings hold it
            // now; one that can't be read fails the edit.
            let families =
                koharu_config::get::<RendererConfig>().map(|config| config.font_families);
            let look = Snapshot::read(families)?;
            let session = app.session()?;
            let mut session = session.lock();
            set(&mut session, block, text, &target, &look)
        })
        .await??,
    )
}

/// Applies to `session` the edit setting `block`'s translation to `text`,
/// a first one in `target`'s language with `look`, the default look, and
/// the marker of a translation typed by hand
/// (`booharu::translation::edit`), as one undo step.
fn set(
    session: &mut Session,
    block: LayerId,
    text: String,
    target: &LanguageIdentifier,
    look: &Snapshot,
) -> anyhow::Result<Change> {
    let edit = booharu::translation::edit(&session.project(), block, text, target, look)?;
    tracing::info!(label = %edit.label, ops = edit.ops.len(), "edit");
    Ok(session.apply(edit)?)
}

#[cfg(test)]
mod tests {
    use anyhow::Context as _;
    use booharu_core::attrs::{self, Auto};
    use koharu_scene::{Content, Created, Edit, Op};
    use serde_json::json;

    use super::*;

    #[test]
    fn a_first_translation_set_in_the_story_panel_is_marked_typed_by_hand_in_its_step()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let mut session = Session::create(dir.path().join("test.khr"))?;
        let add = [
            json!({"op": "add_page", "id": "$p", "index": 0,
                "size": {"width": 400, "height": 400}}),
            json!({"op": "add_layer", "id": "$b", "page": "$p", "index": 0, "layer": {
                "kind": "block", "source": {"content": "どうして", "frame": {"box": {"bounds":
                    {"x": 300.0, "y": 20.0, "width": 40.0, "height": 120.0}}}}}}),
        ];
        let change = session.apply(Edit {
            label: "test".to_owned(),
            ops: add
                .into_iter()
                .map(serde_json::from_value::<Op>)
                .collect::<std::result::Result<_, _>>()?,
        })?;
        let block = change
            .created
            .iter()
            .find_map(|created| match created {
                Created::Layer { id, .. } => Some(*id),
                Created::Page { .. } => None,
            })
            .context("the block")?;
        let english: LanguageIdentifier = "en".parse()?;
        let steps = session.history().steps.len();
        set(
            &mut session,
            block,
            "Why?".to_owned(),
            &english,
            &Snapshot::default(),
        )?;
        // The text and the marker, in one step.
        assert_eq!(session.history().steps.len(), steps + 1);
        let marker = Auto {
            typed: true,
            ..Auto::default()
        };
        assert_eq!(
            attrs::get::<Auto>(session.project().layer(block)?),
            Some(marker)
        );
        // Undone, both go.
        session.undo()?.context("a step")?;
        let project = session.project();
        let layer = project.layer(block)?;
        let Content::Block(found) = &layer.content else {
            anyhow::bail!("{block} isn't a block");
        };
        assert!(found.translation.is_none());
        assert_eq!(attrs::get::<Auto>(layer), None);
        Ok(())
    }
}
