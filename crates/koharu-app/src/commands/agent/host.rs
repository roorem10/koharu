use std::{future::Future, pin::Pin, sync::OnceLock};

use anyhow::{Context as _, Result, anyhow, bail};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use koharu_agent::{Control, Host, Invocation, Tool, ToolCall};
use koharu_desktop::{Desktop, Frame};
use koharu_pipeline::{Committer, Operation, RunStatus, Scope, Stage, StageOutput, StopToken};
use koharu_scene::{Commit, EntityId, Snapshot};
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager as _};
use tauri_runtime_cef::CefRuntime;

use crate::commands::{
    ChannelExt as _,
    canvas::{CanvasChannel, Point},
    editing::{GeometryUpdate, TypographyUpdate},
    output,
    preferences::Preferences,
    processing::{JobId, Processing},
    project::{CurrentProject, Project, Typography},
};

mod tools;

#[derive(Clone)]
pub(crate) struct KoharuHost {
    handle: AppHandle<CefRuntime>,
}

impl KoharuHost {
    pub(crate) fn new(handle: AppHandle<CefRuntime>) -> Self {
        Self { handle }
    }

    async fn project_context(&self) -> Result<Value> {
        let (project, mut pages, context, glossary) = {
            let current = self.handle.state::<CurrentProject>();
            let current = current.project.lock().await;
            let project = current.as_ref().context("no project is open")?;
            let snapshot = project.snapshot();
            let pages = agent_pages(&snapshot)?;
            let context = project.load_context()?;
            let glossary = super::super::context::translation_hints(&context, &snapshot)?.glossary;
            let mut sources = Vec::new();
            for page in snapshot.pages() {
                if let Some(group) = page.text_group()? {
                    for layer in group.text_layers()? {
                        if let Some(source) = layer.content()?.source()? {
                            sources.push(source.text.value);
                        }
                    }
                }
            }
            let glossary = koharu_pipeline::page_glossary(&glossary, &sources);
            (project.info(), pages, context, glossary)
        };
        for page in &mut pages {
            if let Some(layers) = page["layers"].as_array_mut() {
                for layer in layers {
                    if let Ok(id) = serde_json::from_value::<EntityId>(layer["id"].clone()) {
                        layer["lettering_category"] = serde_json::to_value(context.category(id))?;
                        layer["flags"] = serde_json::to_value(
                            context
                                .lines
                                .get(&id)
                                .map(|line| &line.flags)
                                .cloned()
                                .unwrap_or_default(),
                        )?;
                    }
                }
            }
        }
        let preferences = Preferences::load()?;
        let fonts = self
            .handle
            .state::<Desktop>()
            .renderer()
            .available_fonts()
            .await?;
        let providers = preferences
            .providers
            .entries
            .iter()
            .map(|provider| {
                json!({
                    "name": provider.name,
                    "config": provider.config,
                    "credential_configured": provider
                        .credential
                        .as_ref()
                        .is_some_and(|credential| credential.configured),
                })
            })
            .collect::<Vec<_>>();
        let styles = crate::commands::styles::load_text_styles()?.read()?.clone();
        Ok(json!({
            "project": project,
            "pages": pages,
            "brief": context.brief,
            "series_glossary": glossary,
            "configuration": {
                "pipeline": preferences.pipeline,
                "providers": providers,
                "typesetting": preferences.typesetting,
                "text_styles": text_styles_context(&styles),
                "available_fonts": fonts.into_iter().map(|font| font.name).collect::<Vec<_>>(),
            },
        }))
    }

    async fn mutate<T>(
        &self,
        mutation: impl for<'project> FnOnce(
            &'project mut Project,
        ) -> Pin<
            Box<dyn Future<Output = Result<(Commit, T)>> + Send + 'project>,
        > + Send,
    ) -> Result<Invocation>
    where
        T: serde::Serialize + Send + 'static,
    {
        self.mutate_optional(|project| {
            let pending = mutation(project);
            Box::pin(async move {
                let (commit, value) = pending.await?;
                Ok((Some(commit), value))
            })
        })
        .await
    }

    async fn mutate_optional<T>(
        &self,
        mutation: impl for<'project> FnOnce(
            &'project mut Project,
        ) -> Pin<
            Box<dyn Future<Output = Result<(Option<Commit>, T)>> + Send + 'project>,
        > + Send,
    ) -> Result<Invocation>
    where
        T: serde::Serialize + Send + 'static,
    {
        let (commit, value, page, project) = {
            let current = self.handle.state::<CurrentProject>();
            let mut current = current.project.lock().await;
            let project = current.as_mut().context("no project is open")?;
            let (commit, value) = mutation(project).await?;
            let Some(commit) = commit else {
                return Invocation::read(value);
            };
            project.record_commit(&commit);
            project.reconcile_page();
            (commit, value, project.active_page(), project.info())
        };
        let revision = commit.revision;
        self.synchronize(commit, page).await?;
        Invocation::changed(json!({
            "revision": revision,
            "project": project,
            "result": value,
        }))
    }

    async fn synchronize(&self, commit: Commit, page: Option<EntityId>) -> Result<()> {
        let desktop = self.handle.state::<Desktop>();
        desktop.synchronize(&commit.snapshot, page, &commit).await?;
        let canvas = desktop.canvas_state();
        self.handle.state::<CanvasChannel>().channel.publish(canvas);
        Ok(())
    }
}

#[async_trait]
impl Host for KoharuHost {
    async fn context(&self) -> Result<Value> {
        self.project_context().await
    }

    fn tools(&self) -> Vec<Tool> {
        static TOOLS: OnceLock<Vec<Tool>> = OnceLock::new();
        TOOLS.get_or_init(tools::all).clone()
    }

    async fn invoke(&self, call: ToolCall, control: &Control) -> Result<Invocation> {
        tools::invoke(self, &call, control).await
    }
}

fn agent_pages(snapshot: &Snapshot) -> Result<Vec<Value>> {
    Project::pages(snapshot)?
        .into_iter()
        .map(|page| {
            let mut page = serde_json::to_value(Project::page(snapshot, page.id)?)?;
            for layer in page["layers"]
                .as_array_mut()
                .context("page layers must be an array")?
            {
                if layer["type"] != "text" {
                    continue;
                }
                let id = serde_json::from_value(layer["id"].clone())?;
                let info = koharu_pipeline::bubble_shape::text_bubble_shape(snapshot, id)?;
                layer["bubble_shape"] = match (info.bubble, info.shape) {
                    (None, _) => json!("none"),
                    (Some(_), None) => json!("unknown"),
                    (_, Some(shape)) => serde_json::to_value(shape)?,
                };
                layer["shape_measures"] = serde_json::to_value(info.measures)?;
                layer["style_origin"] = json!(if snapshot
                    .component::<koharu_scene::Typography>(id)?
                    .is_some_and(|typography| typography.origin == koharu_scene::Origin::User)
                {
                    "user"
                } else {
                    "generated"
                });
            }
            Ok(page)
        })
        .collect()
}

fn text_styles_context(styles: &koharu_renderer::TextStyles) -> Value {
    json!({ "default_style": styles.default_style, "shape_styles": styles.shape_styles })
}

fn definition<T: JsonSchema>(name: &str, description: &str) -> Tool {
    Tool::new(
        name,
        description,
        serde_json::to_value(schema_for!(T)).expect("tool argument schema must serialize"),
    )
}

fn arguments<T: for<'de> Deserialize<'de>>(call: &ToolCall) -> Result<T> {
    serde_json::from_str(&call.arguments)
        .with_context(|| format!("invalid arguments for {}", call.name))
}

fn entity(value: &str) -> Result<EntityId> {
    serde_json::from_value(Value::String(value.to_owned()))
        .with_context(|| format!("invalid entity ID {value}"))
}

fn entities(values: &[String]) -> Result<Vec<EntityId>> {
    values.iter().map(|value| entity(value)).collect()
}
