use anyhow::{Context as _, Result};
use koharu_agent::Invocation;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use tauri::Manager as _;

use crate::commands::{
    agent::host::{KoharuHost, entity},
    project::CurrentProject,
};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct GetBoxNote {
    layer: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetBoxNote {
    layer: String,
    note: Option<String>,
}

impl KoharuHost {
    pub(crate) async fn get_box_note(&self, args: GetBoxNote) -> Result<Invocation> {
        let layer = entity(&args.layer)?;
        let current = self.handle.state::<CurrentProject>();
        let current = current.project.lock().await;
        let project = current.as_ref().context("no project is open")?;
        Invocation::read(json!({ "layer": layer, "note": project.box_note(layer)? }))
    }

    pub(crate) async fn set_box_note(&self, args: SetBoxNote) -> Result<Invocation> {
        let layer = entity(&args.layer)?;
        self.mutate_optional(|project| {
            Box::pin(async move {
                let commit = project.set_box_note(layer, args.note).await?;
                Ok((commit, json!({ "layer": layer })))
            })
        })
        .await
    }
}
