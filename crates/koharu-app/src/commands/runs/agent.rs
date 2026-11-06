use anyhow::{Context as _, Result};
use koharu_agent::{Invocation, Tool, ToolCall};
use koharu_pipeline::{OnPageFailure, Operation, RunOrder, Scope};
use koharu_scene::EntityId;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Manager as _};
use tauri_runtime_cef::CefRuntime;

use super::{RunPreset, RunSettings};
use crate::commands::{
    processing::{self, JobChannel, JobId, Processing},
    project::CurrentProject,
};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Empty {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SaveSettings {
    retries: Option<u8>,
    #[schemars(with = "Option<FailureMode>")]
    on_page_failure: Option<OnPageFailure>,
    #[schemars(with = "Option<StageOrder>")]
    order: Option<RunOrder>,
    finish_sound: Option<bool>,
    #[schemars(with = "Option<Vec<Preset>>")]
    presets: Option<Vec<RunPreset>>,
}

// The native types own parsing and validation; these describe their wire values to the agent.
#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
enum FailureMode {
    ContinueWithOtherPages,
    StopRun,
}

#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
enum StageOrder {
    Overlapped,
    StageAtATime,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct Preset {
    name: String,
    stages: Vec<Stage>,
}

#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
enum Stage {
    Detection,
    Ocr,
    Translation,
    Inpainting,
}

impl SaveSettings {
    fn apply(self, mut settings: RunSettings) -> RunSettings {
        if let Some(retries) = self.retries {
            settings.retries = retries;
        }
        if let Some(on_page_failure) = self.on_page_failure {
            settings.on_page_failure = on_page_failure;
        }
        if let Some(order) = self.order {
            settings.order = order;
        }
        if let Some(finish_sound) = self.finish_sound {
            settings.finish_sound = finish_sound;
        }
        if let Some(presets) = self.presets {
            settings.presets = presets;
        }
        settings
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RunPresetArgs {
    /// Zero-based index from get_run_settings.
    preset: u32,
    /// Nonempty page selection; omit for project scope.
    #[schemars(with = "Option<Vec<String>>")]
    pages: Option<Vec<EntityId>>,
    /// Nonempty text-layer selection, mutually exclusive with pages.
    #[schemars(with = "Option<Vec<String>>")]
    elements: Option<Vec<EntityId>>,
}

impl RunPresetArgs {
    fn request(self, settings: &RunSettings) -> Result<(Scope, Operation)> {
        let stages = settings
            .presets
            .get(self.preset as usize)
            .context("choose a saved run preset")?
            .stages
            .clone();
        let scope = match (self.pages, self.elements) {
            (None, None) => Scope::Project,
            (Some(pages), None) => {
                anyhow::ensure!(!pages.is_empty(), "page scope is empty");
                Scope::Pages(pages)
            }
            (None, Some(elements)) => {
                anyhow::ensure!(!elements.is_empty(), "element scope is empty");
                Scope::Entities(elements)
            }
            (Some(_), Some(_)) => anyhow::bail!("choose pages or elements, not both"),
        };
        Ok((scope, Operation::Stages { stages }))
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct JobArgs {
    #[schemars(with = "String")]
    job: JobId,
}

fn definition<T: JsonSchema>(name: &str, description: &str) -> Tool {
    Tool::new(
        name,
        description,
        serde_json::to_value(schema_for!(T)).expect("tool argument schema must serialize"),
    )
}

pub(crate) fn get_settings_definition() -> Tool {
    definition::<Empty>(
        "get_run_settings",
        "Read saved run presets, retry limits, failure policy and stage order.",
    )
}

pub(crate) fn save_settings_definition() -> Tool {
    definition::<SaveSettings>(
        "save_run_settings",
        "Change supplied run settings; omitted fields stay unchanged. Presets replaces the saved list. Applies to future runs.",
    )
}

pub(crate) fn list_runs_definition() -> Tool {
    definition::<Empty>(
        "list_runs",
        "Read run IDs, progress, failures and whether a chapter batch is active.",
    )
}

pub(crate) fn run_preset_definition() -> Tool {
    definition::<RunPresetArgs>(
        "run_preset",
        "Start a saved preset by zero-based index. Choose pages or elements; omit both for the project. Returns a job ID for list_runs.",
    )
}

pub(crate) fn retry_definition() -> Tool {
    definition::<JobArgs>(
        "retry_failed_pages",
        "Retry a completed run's failed pages in the current project using saved run settings.",
    )
}

pub(crate) fn stop_definition() -> Tool {
    definition::<JobArgs>(
        "stop_job",
        "Cooperatively stop a running job; discard uncommitted work.",
    )
}

fn arguments<T: for<'de> Deserialize<'de>>(call: &ToolCall) -> Result<T> {
    serde_json::from_str(&call.arguments)
        .with_context(|| format!("invalid arguments for {}", call.name))
}

pub(crate) async fn invoke(handle: &AppHandle<CefRuntime>, call: &ToolCall) -> Result<Invocation> {
    match call.name.as_str() {
        "get_run_settings" => {
            let _: Empty = arguments(call)?;
            Invocation::read(super::get_run_settings().await.map_err(|error| error.0)?)
        }
        "save_run_settings" => {
            let args: SaveSettings = arguments(call)?;
            let settings = super::get_run_settings().await.map_err(|error| error.0)?;
            Invocation::changed(
                super::save_run_settings(args.apply(settings))
                    .await
                    .map_err(|error| error.0)?,
            )
        }
        "list_runs" => {
            let _: Empty = arguments(call)?;
            let jobs = handle
                .state::<Processing>()
                .jobs
                .lock()
                .values()
                .cloned()
                .collect::<Vec<_>>();
            Invocation::read(json!({
                "jobs": jobs,
                "chapter_batch_active": handle.state::<super::BatchState>().active(),
            }))
        }
        "run_preset" => {
            let args: RunPresetArgs = arguments(call)?;
            let settings = super::get_run_settings().await.map_err(|error| error.0)?;
            let (scope, operation) = args.request(&settings)?;
            let job = processing::process(
                handle.clone(),
                scope,
                operation,
                handle.state::<CurrentProject>(),
                handle.state::<Processing>(),
                handle.state::<JobChannel>(),
            )
            .await
            .map_err(|error| error.0)?;
            Invocation::changed(json!({ "job": job }))
        }
        "retry_failed_pages" => {
            let args: JobArgs = arguments(call)?;
            let job = super::retry_failed_pages(args.job, handle.clone())
                .await
                .map_err(|error| error.0)?;
            Invocation::changed(json!({ "job": job }))
        }
        "stop_job" => {
            let args: JobArgs = arguments(call)?;
            processing::stop_job(args.job, handle.state::<Processing>())
                .await
                .map_err(|error| error.0)?;
            Invocation::changed(json!({ "job": args.job, "stop_requested": true }))
        }
        _ => anyhow::bail!("unknown run tool {}", call.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_policy_preserves_saved_presets_and_other_settings() {
        let mut current = RunSettings::default();
        current.retries = 4;
        current.finish_sound = false;
        current.presets = vec![RunPreset {
            name: "Custom".into(),
            stages: vec![koharu_pipeline::Stage::Ocr],
        }];
        let args: SaveSettings = serde_json::from_value(json!({
            "order": "stage_at_a_time",
            "on_page_failure": "stop_run",
        }))
        .unwrap();
        let updated = args.apply(current);
        updated.validate().unwrap();
        assert_eq!(updated.order, RunOrder::StageAtATime);
        assert_eq!(updated.on_page_failure, OnPageFailure::StopRun);
        assert_eq!(updated.retries, 4);
        assert!(!updated.finish_sound);
        assert_eq!(updated.presets.len(), 1);
        assert_eq!(updated.presets[0].stages, [koharu_pipeline::Stage::Ocr]);
        let invalid: SaveSettings = serde_json::from_value(json!({ "retries": 6 })).unwrap();
        assert!(invalid.apply(updated).validate().is_err());
    }

    #[test]
    fn preset_uses_saved_stages_without_broadening_page_selection() {
        let settings = RunSettings::default();
        let page = EntityId::new();
        let args: RunPresetArgs = serde_json::from_value(json!({
            "preset": 0,
            "pages": [page],
        }))
        .unwrap();
        let (scope, operation) = args.request(&settings).unwrap();
        assert_eq!(scope, Scope::Pages(vec![page]));
        assert_eq!(operation.stages().unwrap(), settings.presets[0].stages);
        let project: RunPresetArgs = serde_json::from_value(json!({ "preset": 0 })).unwrap();
        assert_eq!(project.request(&settings).unwrap().0, Scope::Project);
        for selection in [
            json!({ "pages": [] }),
            json!({ "elements": [] }),
            json!({ "pages": [], "elements": [] }),
            json!({ "pages": [page], "elements": [] }),
            json!({ "pages": [], "elements": [EntityId::new()] }),
            json!({ "pages": [page], "elements": [EntityId::new()] }),
        ] {
            let mut arguments = selection;
            arguments["preset"] = json!(0);
            let invalid: RunPresetArgs = serde_json::from_value(arguments).unwrap();
            assert!(invalid.request(&settings).is_err());
        }
        let missing: RunPresetArgs = serde_json::from_value(json!({ "preset": 100 })).unwrap();
        assert!(missing.request(&settings).is_err());
    }
}
