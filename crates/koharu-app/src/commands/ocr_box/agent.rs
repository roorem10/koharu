use anyhow::Result;
use koharu_agent::Invocation;
use koharu_pipeline::Bounds;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use tauri::Manager as _;

use super::{RedoAction, settings::OcrBoxMode};
use crate::commands::{
    agent::host::{KoharuHost, entities, entity},
    project::CurrentProject,
};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddOcrBox {
    pub page: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Omit to use the saved OCR box mode.
    pub mode: Option<OcrBoxMode>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OcrLayers {
    pub layers: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RedoOcrBoxes {
    pub layers: Vec<String>,
    pub action: RedoAction,
    /// Set only when the user asks to replace hand-edited text.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddSfxSubtitle {
    pub layer: String,
}

impl KoharuHost {
    pub(crate) async fn agent_add_ocr_box(&self, args: AddOcrBox) -> Result<Invocation> {
        let page = entity(&args.page)?;
        let mode = match args.mode {
            Some(mode) => mode,
            None => super::settings::OcrBoxSettings::load()?.default_mode,
        };
        let layer = super::add_ocr_box(
            self.handle.clone(),
            page,
            Bounds {
                x: args.x,
                y: args.y,
                width: args.width,
                height: args.height,
            },
            mode,
        )
        .await
        .map_err(|error| error.0)?;
        Invocation::changed(json!({ "layer": layer, "processing_started": true }))
    }

    pub(crate) async fn agent_get_ocr_redo_info(&self, args: OcrLayers) -> Result<Invocation> {
        let info =
            super::get_ocr_redo_info(entities(&args.layers)?, self.handle.state::<CurrentProject>())
                .await
                .map_err(|error| error.0)?;
        Invocation::read(info)
    }

    pub(crate) async fn agent_redo_ocr_boxes(&self, args: RedoOcrBoxes) -> Result<Invocation> {
        let jobs = super::redo_ocr_boxes(
            self.handle.clone(),
            entities(&args.layers)?,
            args.action,
            args.overwrite,
        )
        .await
        .map_err(|error| error.0)?;
        Invocation::changed(json!({ "jobs": jobs, "processing_started": true }))
    }

    pub(crate) async fn agent_add_sfx_subtitle(&self, args: AddSfxSubtitle) -> Result<Invocation> {
        let layer = super::add_sfx_subtitle(self.handle.clone(), entity(&args.layer)?)
            .await
            .map_err(|error| error.0)?;
        Invocation::changed(json!({ "layer": layer }))
    }
}
