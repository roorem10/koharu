//! A block's translation set by the agent as the Story panel sets one: a
//! block without a translation of its own, none or only the placeholder a
//! style set before translating leaves, gets its first translation where
//! Koharu's scene starts one, in its source's place, in the same undo step
//! as the target language, its clean-up shown, the default look a new
//! translation gets where the placeholder set no value of its own, and the
//! marker of a translation typed by hand, so once cleared it stays a hand
//! edit.

use booharu_core::settings::Snapshot;
use koharu_pipeline::PipelineConfig;
use koharu_renderer::RendererConfig;
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
         of its own, none or only the empty placeholder a style set on the box before it was \
         translated leaves, gets its first translation as typing on the canvas starts one: in \
         its source's place, frame and lettering, in the translator's target language, with \
         the block's clean-up shown under it. It keeps the values set on the placeholder, and \
         gets the default look new translations get for the rest: the first Preferred font, \
         or while Pick fonts automatically is on its lettering category's font, and with \
         colour detection off the default colours. A block with a translation of its own, \
         blank or not, keeps its place, lettering, and language. A translation set this way \
         counts as typed by hand, also once cleared or set blank, so translation runs leave it \
         alone unless booharu_redo_boxes takes it, and booharu_apply_text_style leaves it alone \
         unless told to include such blocks. Answers what changed."
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
        // first translation is written only where the edit finds none.
        tokio::task::spawn_blocking(move || -> anyhow::Result<Change> {
            let target = koharu_config::get::<PipelineConfig>()?.translator.target;
            let families =
                koharu_config::get::<RendererConfig>().map(|config| config.font_families);
            let look = Snapshot::read(families)?;
            let mut session = tools.session.lock();
            let edit = booharu::translation::edit(
                &session.project(),
                args.block,
                args.text,
                &target,
                &look,
            )?;
            Ok(session.apply(edit)?)
        })
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
    }
}
