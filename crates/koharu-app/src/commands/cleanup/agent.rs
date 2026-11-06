use anyhow::Result;
use koharu_agent::{Invocation, ToolCall};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use tauri::Manager as _;

use super::{CleanupRect, CleanupStroke};
use crate::commands::{
    agent::host::{KoharuHost, arguments, entity},
    project::{CurrentProject, RasterStrokeMode},
};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CleanAreas {
    page: String,
    #[serde(default)]
    strokes: Vec<CleanupStroke>,
    #[serde(default)]
    rects: Vec<CleanupRect>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PaintCleanup {
    page: String,
    layer: Option<String>,
    stroke: CleanupStroke,
    color: [u8; 4],
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct EraseCleanup {
    page: String,
    layer: Option<String>,
    stroke: CleanupStroke,
}

impl KoharuHost {
    pub(crate) async fn clean_areas_tool(&self, call: &ToolCall) -> Result<Invocation> {
        let args: CleanAreas = arguments(call)?;
        let page = entity(&args.page)?;
        let job = super::clean_areas(
            page,
            args.strokes,
            args.rects,
            self.handle.clone(),
            self.handle.state::<CurrentProject>(),
        )
        .await
        .map_err(|error| error.0)?;
        Invocation::changed(json!({ "page": page, "job": job, "started": true }))
    }

    pub(crate) async fn paint_cleanup_tool(&self, call: &ToolCall) -> Result<Invocation> {
        let args: PaintCleanup = arguments(call)?;
        let page = entity(&args.page)?;
        let layer = args.layer.as_deref().map(entity).transpose()?;
        self.mutate(|project| {
            Box::pin(async move {
                let (commit, layer) = project
                    .apply_raster_stroke(
                        page,
                        layer,
                        RasterStrokeMode::Paint,
                        args.color,
                        args.stroke.diameter,
                        args.stroke.hardness,
                        args.stroke
                            .points
                            .into_iter()
                            .map(|point| koharu_scene::Point {
                                x: point.x,
                                y: point.y,
                            })
                            .collect(),
                    )
                    .await?;
                Ok((commit, json!({ "page": page, "layer": layer })))
            })
        })
        .await
    }

    pub(crate) async fn erase_cleanup_tool(&self, call: &ToolCall) -> Result<Invocation> {
        let args: EraseCleanup = arguments(call)?;
        let page = entity(&args.page)?;
        let layer = args.layer.as_deref().map(entity).transpose()?;
        self.mutate(|project| {
            Box::pin(async move {
                let (commit, layer) = project
                    .apply_raster_stroke(
                        page,
                        layer,
                        RasterStrokeMode::Erase,
                        [0; 4],
                        args.stroke.diameter,
                        args.stroke.hardness,
                        args.stroke
                            .points
                            .into_iter()
                            .map(|point| koharu_scene::Point {
                                x: point.x,
                                y: point.y,
                            })
                            .collect(),
                    )
                    .await?;
                Ok((commit, json!({ "page": page, "layer": layer })))
            })
        })
        .await
    }
}
