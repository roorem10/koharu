use anyhow::{Context as _, Result};
use koharu_agent::Invocation;
use koharu_scene::TextWarp;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use tauri::Manager as _;

use crate::commands::{
    agent::host::{KoharuHost, entities},
    project::CurrentProject,
};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct GetTextWarp {
    layers: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetTextWarp {
    layers: Vec<String>,
    #[schemars(with = "Option<WarpSchema>")]
    warp: Option<TextWarp>,
}

// Scene owns decoding and validation; this mirrors its wire shape for tool discovery.
#[derive(JsonSchema)]
#[allow(dead_code)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WarpSchema {
    Skew { x_degrees: f32, y_degrees: f32 },
    /// Frame-local corners clockwise from top-left; the unit square is unchanged.
    Perspective { corners: [[f32; 2]; 4] },
}

impl KoharuHost {
    pub(crate) async fn get_text_warp(&self, args: GetTextWarp) -> Result<Invocation> {
        let layers = entities(&args.layers)?;
        let current = self.handle.state::<CurrentProject>();
        let current = current.project.lock().await;
        let project = current.as_ref().context("no project is open")?;
        let warps = project.text_warps(&layers)?;
        Invocation::read(
            layers
                .into_iter()
                .zip(warps)
                .map(|(layer, warp)| json!({ "layer": layer, "warp": warp }))
                .collect::<Vec<_>>(),
        )
    }

    pub(crate) async fn set_text_warp(&self, args: SetTextWarp) -> Result<Invocation> {
        let layers = entities(&args.layers)?;
        self.mutate(|project| {
            Box::pin(async move {
                let commit = project.set_text_warp(layers.clone(), args.warp).await?;
                Ok((commit, json!({ "layers": layers })))
            })
        })
        .await
    }
}
