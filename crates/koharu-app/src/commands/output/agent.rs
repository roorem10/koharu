use std::path::PathBuf;

use anyhow::{Context as _, Result, bail, ensure};
use koharu_agent::{Invocation, Tool, ToolCall};
use koharu_desktop::Desktop;
use koharu_scene::EntityId;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Manager as _};
use tauri_runtime_cef::CefRuntime;

use super::{ExportFormat, prepare_export, write_export};
use crate::commands::{
    canvas::CanvasChannel,
    files::{
        self,
        scripts::{self, ScriptPreview},
    },
    project::CurrentProject,
};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExportPages {
    format: ExportFormat,
    destination: String,
    pages: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ScriptPath {
    path: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ApplyScriptImport {
    preview: ScriptPreview,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetExportNaming {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetExportNaming {
    pattern: String,
}

fn definition<T: JsonSchema>(name: &str, description: &str) -> Tool {
    Tool::new(
        name,
        description,
        serde_json::to_value(schema_for!(T)).expect("tool argument schema must serialize"),
    )
}

pub(crate) fn export_pages_definition() -> Tool {
    definition::<ExportPages>(
        "export_pages",
        "Export pages in project order. PNG/PSD destination is an existing directory; CBZ is a file. Use an absolute path. Omit pages for all pages.",
    )
}

pub(crate) fn export_script_definition() -> Tool {
    definition::<ScriptPath>(
        "export_script",
        "Export the translation script to an absolute file path.",
    )
}

pub(crate) fn preview_script_import_definition() -> Tool {
    definition::<ScriptPath>(
        "preview_script_import",
        "Preview translation changes from an absolute script file path; does not edit the project.",
    )
}

pub(crate) fn apply_script_import_definition() -> Tool {
    definition::<ApplyScriptImport>(
        "apply_script_import",
        "Apply a script import preview in one undo step. Rejects a stale preview.",
    )
}

pub(crate) fn get_export_naming_definition() -> Tool {
    definition::<GetExportNaming>("get_export_naming", "Read the export filename pattern.")
}

pub(crate) fn set_export_naming_definition() -> Tool {
    definition::<SetExportNaming>(
        "set_export_naming",
        "Set the export filename pattern. Tokens: {project}, {index} (padded), {number}, {page}.",
    )
}

fn arguments<T: for<'de> Deserialize<'de>>(call: &ToolCall) -> Result<T> {
    serde_json::from_str(&call.arguments)
        .with_context(|| format!("invalid arguments for {}", call.name))
}

fn absolute_path(value: String) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    ensure!(path.is_absolute(), "path must be absolute");
    Ok(path)
}

pub(crate) async fn dispatch(handle: &AppHandle<CefRuntime>, call: &ToolCall) -> Result<Invocation> {
    match call.name.as_str() {
        "export_pages" => {
            let args: ExportPages = arguments(call)?;
            let destination = absolute_path(args.destination)?;
            if matches!(args.format, ExportFormat::Png | ExportFormat::Psd) {
                ensure!(
                    destination.is_dir(),
                    "destination must be an existing directory"
                );
            }
            let pages = args
                .pages
                .map(|pages| {
                    pages
                        .into_iter()
                        .map(|page| {
                            serde_json::from_value::<EntityId>(json!(page)).context("invalid page ID")
                        })
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?;
            let current = handle.state::<CurrentProject>();
            let plan = prepare_export(args.format, pages, &current).await?;
            let files = if matches!(args.format, ExportFormat::Cbz) {
                vec![destination.clone()]
            } else {
                plan.names.iter().map(|name| destination.join(name)).collect()
            };
            write_export(plan, args.format, destination, &handle.state::<Desktop>()).await?;
            Invocation::read(json!({"files": files}))
        }
        "export_script" | "preview_script_import" => {
            let args: ScriptPath = arguments(call)?;
            let path = absolute_path(args.path)?;
            let (snapshot, name) = {
                let current = handle.state::<CurrentProject>();
                let current = current.project.lock().await;
                let project = current.as_ref().context("no project is open")?;
                (project.snapshot(), project.name.clone())
            };
            if call.name == "export_script" {
                scripts::export_script_to(&path, &snapshot, &name).await?;
                Invocation::read(json!({"path": path}))
            } else {
                Invocation::read(scripts::preview_script_file(&path, &snapshot, &name).await?)
            }
        }
        "apply_script_import" => {
            let args: ApplyScriptImport = arguments(call)?;
            let changed = args.preview.changes.len();
            scripts::apply_script_import(
                args.preview,
                handle.state::<CurrentProject>(),
                handle.state::<Desktop>(),
                handle.state::<CanvasChannel>(),
            )
            .await
            .map_err(|error| error.0)?;
            if changed == 0 {
                Invocation::read(json!({"imported": changed}))
            } else {
                Invocation::changed(json!({"imported": changed}))
            }
        }
        "get_export_naming" => {
            let _: GetExportNaming = arguments(call)?;
            Invocation::read(json!({"pattern": files::load_settings()?.export_name_pattern}))
        }
        "set_export_naming" => {
            let args: SetExportNaming = arguments(call)?;
            let mut settings = files::load_settings()?;
            settings.export_name_pattern = args.pattern;
            let settings = files::save_library_settings(settings).map_err(|error| error.0)?;
            Invocation::changed(json!({"pattern": settings.export_name_pattern}))
        }
        _ => bail!("unknown output tool {}", call.name),
    }
}
