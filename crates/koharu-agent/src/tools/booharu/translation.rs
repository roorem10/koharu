//! A block's translation set by the agent as the Story panel sets one: a
//! block without a translation gets it placed into its own frame first, in
//! the same undo step, where the scene's `set_text` alone would leave it at
//! the page's corner.

use koharu_pipeline::PipelineConfig;
use koharu_scene::{Change, LayerId};
use rig_core::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::tools::{failed, project, schema};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SetTranslationArgs {
    /// The block.
    block: LayerId,
    /// The translation's whole text.
    text: String,
}

pub(super) struct SetTranslation;

impl Tool for SetTranslation {
    const NAME: &'static str = "booharu_set_translation";
    type Args = SetTranslationArgs;
    type Output = Change;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Set a block's translation to a text, as one undo step. A block without a translation \
         gets one placed first, as the Translation stage places its own in the translator's \
         target language: in the block's room in its balloon, or in its source's frame, at the \
         page's lettering size, with the block's clean-up shown under it. A block with one keeps \
         its place, lettering, and language. Answers what changed."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<SetTranslationArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: SetTranslationArgs,
    ) -> Result<Change, Self::Error> {
        let tools = project(context)?;
        // On a blocking thread, as `edit` applies, and under one lock, so a
        // translation is placed only where the edit finds none.
        tokio::task::spawn_blocking(move || -> anyhow::Result<Change> {
            let target = koharu_config::get::<PipelineConfig>()?.translator.target;
            let mut session = tools.session.lock();
            let edit =
                booharu::translation::edit(&session.project(), args.block, args.text, &target)?;
            Ok(session.apply(edit)?)
        })
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
    }
}
