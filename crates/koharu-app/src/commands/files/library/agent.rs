use anyhow::{Context as _, Result, bail};
use koharu_agent::{Invocation, Tool, ToolCall};
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Manager as _};
use tauri_runtime_cef::CefRuntime;

use crate::commands::{
    lifecycle,
    project::{CurrentProject, ProjectLibrary},
};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListProjects {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RenameProject {
    name: String,
    new_name: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DuplicateProject {
    name: String,
}

pub(crate) fn list_definition() -> Tool {
    Tool::new(
        "list_projects",
        "List projects in the configured library.",
        json!(schema_for!(ListProjects)),
    )
}

pub(crate) fn rename_definition() -> Tool {
    Tool::new(
        "rename_project",
        "Rename a closed library project; existing names are refused.",
        json!(schema_for!(RenameProject)),
    )
}

pub(crate) fn duplicate_definition() -> Tool {
    Tool::new(
        "duplicate_project",
        "Duplicate a saved library project under a unique name.",
        json!(schema_for!(DuplicateProject)),
    )
}

pub(crate) async fn invoke(handle: &AppHandle<CefRuntime>, call: &ToolCall) -> Result<Invocation> {
    // Opening uses lifecycle::replace_project, which waits for AgentState::reset.
    // An agent invoking it would wait for its own run; browser drafts also have no
    // native flush barrier. Keep project switching in the UI until both are owned.
    match call.name.as_str() {
        "list_projects" => {
            let _: ListProjects = serde_json::from_str(&call.arguments)
                .context("invalid arguments for list_projects")?;
            Invocation::read(
                lifecycle::list_projects(handle.state::<ProjectLibrary>())
                    .await
                    .map_err(|error| error.0)?,
            )
        }
        "rename_project" => {
            let args: RenameProject = serde_json::from_str(&call.arguments)
                .context("invalid arguments for rename_project")?;
            super::rename_project(
                args.name,
                args.new_name.clone(),
                handle.state::<CurrentProject>(),
                handle.state::<ProjectLibrary>(),
            )
            .await
            .map_err(|error| error.0)?;
            Invocation::changed(json!({ "name": args.new_name }))
        }
        "duplicate_project" => {
            let args: DuplicateProject = serde_json::from_str(&call.arguments)
                .context("invalid arguments for duplicate_project")?;
            let name = super::duplicate_project(
                args.name,
                handle.state::<CurrentProject>(),
                handle.state::<ProjectLibrary>(),
            )
            .await
            .map_err(|error| error.0)?;
            Invocation::changed(json!({ "name": name }))
        }
        name => bail!("unknown library tool {name}"),
    }
}
