use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use koharu_agent::Invocation;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use tauri::Manager as _;

use super::RawPrepOptions;
use crate::commands::{
    agent::host::{KoharuHost, entities, entity},
    project::CurrentProject,
};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrepPages {
    pub pages: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreviewPrep {
    pub page: String,
    pub options: RawPrepOptions,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreparePages {
    pub pages: Vec<String>,
    pub options: RawPrepOptions,
}

impl KoharuHost {
    pub(crate) async fn agent_get_prep_page_status(&self, args: PrepPages) -> Result<Invocation> {
        let status = super::get_prep_page_status(
            entities(&args.pages)?,
            self.handle.state::<CurrentProject>(),
        )
        .await
        .map_err(|error| error.0)?;
        Invocation::read(status)
    }

    pub(crate) async fn agent_preview_prep(&self, args: PreviewPrep) -> Result<Invocation> {
        let page = entity(&args.page)?;
        let preview =
            super::preview_prep(page, args.options, self.handle.state::<CurrentProject>())
                .await
                .map_err(|error| error.0)?;
        Ok(Invocation::read(json!({
            "page": page,
            "black": preview.black,
            "white": preview.white,
        }))?
        .with_image(
            format!("Page {page} before preparation"),
            format!("data:image/png;base64,{}", STANDARD.encode(preview.before)),
        )
        .with_image(
            format!("Page {page} after preparation"),
            format!("data:image/png;base64,{}", STANDARD.encode(preview.after)),
        ))
    }

    pub(crate) async fn agent_prepare_pages(&self, args: PreparePages) -> Result<Invocation> {
        let result =
            super::prepare_pages(entities(&args.pages)?, args.options, self.handle.clone())
                .await
                .map_err(|error| error.0)?;
        if result.changed == 0 {
            Invocation::read(result)
        } else {
            Invocation::changed(result)
        }
    }

    pub(crate) async fn agent_restore_original_pages(&self, args: PrepPages) -> Result<Invocation> {
        let result = super::restore_original_pages(entities(&args.pages)?, self.handle.clone())
            .await
            .map_err(|error| error.0)?;
        if result.changed == 0 {
            Invocation::read(result)
        } else {
            Invocation::changed(result)
        }
    }
}
