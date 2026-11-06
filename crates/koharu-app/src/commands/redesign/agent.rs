use anyhow::{Context as _, Result};
use koharu_agent::Invocation;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use tauri::Manager as _;

use super::EffectResolver;
use crate::commands::{
    agent::host::{KoharuHost, entities},
    project::CurrentProject,
    styles::load_text_styles,
};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BoxEffects {
    layers: Vec<String>,
}

impl KoharuHost {
    pub(crate) async fn get_box_effects(&self, args: BoxEffects) -> Result<Invocation> {
        let layers = entities(&args.layers)?;
        let styles = load_text_styles()?.read()?.clone();
        let detect_colors = koharu_pipeline::PipelineConfig::load()?
            .read()?
            .detect_text_color;
        let current = self.handle.state::<CurrentProject>();
        let current = current.project.lock().await;
        let project = current.as_ref().context("no project is open")?;
        let snapshot = project.snapshot();
        let resolver = EffectResolver::new(project, &styles, detect_colors)?;
        let effects = layers
            .into_iter()
            .map(|layer| {
                Ok(json!({
                    "layer": layer,
                    "has_effects": resolver.has_effects(&snapshot, layer)?,
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        Invocation::read(effects)
    }

    pub(crate) async fn reset_box_effects(&self, args: BoxEffects) -> Result<Invocation> {
        let layers = entities(&args.layers)?;
        let styles = load_text_styles()?.read()?.clone();
        let detect_colors = koharu_pipeline::PipelineConfig::load()?
            .read()?
            .detect_text_color;
        self.mutate_optional(|project| {
            Box::pin(async move {
                let commit = project.reset_effects(&layers, &styles, detect_colors).await?;
                Ok((commit, json!({ "layers": layers })))
            })
        })
        .await
    }
}
