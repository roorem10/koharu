//! A reviewer's note on a block or a text, kept in its Booharu attributes as
//! `review_note`, as the app's own command sets one.

use koharu_scene::{Change, LayerId};
use rig_core::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::apply;
use crate::tools::{project, schema};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SetReviewNoteArgs {
    /// The block or text.
    layer: LayerId,
    /// The note's whole text; null, blank, or left out removes the note.
    note: Option<String>,
}

pub(super) struct SetReviewNote;

impl Tool for SetReviewNote {
    const NAME: &'static str = "booharu_set_review_note";
    type Args = SetReviewNoteArgs;
    type Output = Change;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Set the reviewer's note on a block or a text layer, as one undo step, or remove it when \
         the note is null or blank. The note is for whoever reviews the translation and isn't \
         drawn on the page. A locked layer, or one inside a locked layer, refuses it. Answers \
         what changed."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<SetReviewNoteArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: SetReviewNoteArgs,
    ) -> Result<Change, Self::Error> {
        let tools = project(context)?;
        let SetReviewNoteArgs { layer, note } = args;
        apply(&tools, move |session| {
            booharu::boxes::review_note(&session.project(), layer, note)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use booharu_core::attrs::{self, ReviewNote};
    use koharu_pipeline::{Pipeline, PipelineConfig};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::{Created, Edit, Session};
    use parking_lot::Mutex;
    use serde_json::json;

    use super::*;
    use crate::Tools;

    /// Tools on a project of one page holding one text block, and the block.
    fn open() -> (tempfile::TempDir, Tools, LayerId) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let ops = [
            json!({"op": "add_page", "id": "$page", "index": 0, "size": {"width": 100, "height": 100}}),
            json!({"op": "add_layer", "id": "$block", "page": "$page", "index": 0,
                "layer": {"kind": "block", "source": {"content": "やあ"}}}),
        ];
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(|op| serde_json::from_value(op).expect("an op"))
                    .collect(),
            })
            .expect("a block");
        let block = change
            .created
            .iter()
            .find_map(|created| match created {
                Created::Layer { id, .. } => Some(*id),
                Created::Page { .. } => None,
            })
            .expect("the block");
        let tools = Tools::new(
            Arc::new(Mutex::new(session)),
            Pipeline::new(PipelineConfig::default()),
            Renderer::new(RendererConfig::default()),
        );
        (dir, tools, block)
    }

    async fn note(tools: &Tools, layer: LayerId, note: Value) -> Result<Change, String> {
        let args = json!({"layer": layer, "note": note});
        SetReviewNote
            .call(
                &mut tools.tool_context(),
                serde_json::from_value(args).expect("a note"),
            )
            .await
            .map_err(|error| error.to_string())
    }

    fn noted(tools: &Tools, layer: LayerId) -> Option<String> {
        let project = tools.session.lock().project();
        attrs::get::<ReviewNote>(project.layer(layer).expect("the layer")).map(|note| note.0)
    }

    #[tokio::test]
    async fn a_review_note_is_set_and_cleared_but_never_on_a_locked_layer() {
        let (_dir, tools, block) = open();
        note(&tools, block, json!("Check the pun"))
            .await
            .expect("a note");
        assert_eq!(noted(&tools, block).as_deref(), Some("Check the pun"));
        assert_eq!(
            tools
                .session
                .lock()
                .history()
                .steps
                .last()
                .map(String::as_str),
            Some(booharu::boxes::SET_REVIEW_NOTE)
        );
        note(&tools, block, Value::Null).await.expect("no note");
        assert_eq!(noted(&tools, block), None);

        tools
            .session
            .lock()
            .apply(Edit {
                label: "test".to_owned(),
                ops: vec![koharu_scene::Op::UpdateLayer {
                    layer: block.into(),
                    patch: json!({"locked": true}),
                }],
            })
            .expect("a lock");
        let refused = note(&tools, block, json!("Too late")).await;
        assert!(
            refused.as_ref().is_err_and(|error| {
                error.contains(" is locked") && error.contains(&format!("(layer {block})"))
            }),
            "{refused:?}"
        );
        assert_eq!(noted(&tools, block), None);
    }
}
