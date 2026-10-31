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

    async fn context_tool(&self, call: &ToolCall) -> Result<Invocation> {
        let (commit, page, value) = {
            let projects = self.handle.state::<CurrentProject>();
            let mut projects = projects.project.lock().await;
            let project = projects.as_mut().context("no project is open")?;
            let mut changed_layers = Vec::new();
            let value = match call.name.as_str() {
                "add_glossary_entry" => {
                    let args: AddGlossaryEntry = arguments(call)?;
                    anyhow::ensure!(
                        !args.source.trim().is_empty() && !args.target.trim().is_empty(),
                        "term needs source and target"
                    );
                    let context = project.load_context()?;
                    let series = super::super::context::linked_series(&context)?;
                    let kind: booharu_context::TermKind = serde_json::from_value(json!(args.kind))?;
                    let added = project.update_context(|context| {
                        if series
                            .iter()
                            .flat_map(|series| &series.glossary)
                            .chain(&context.suggestions)
                            .any(|entry| entry.source.trim() == args.source.trim())
                        {
                            return false;
                        }
                        context.suggestions.push(booharu_context::GlossaryEntry {
                            id: EntityId::new().to_string(),
                            source: args.source.trim().into(),
                            target: args.target.trim().into(),
                            kind,
                            note: args.note,
                            locked: false,
                        });
                        true
                    })?;
                    json!({"suggested":added})
                }
                "set_line_category" => {
                    let args: SetLineCategory = arguments(call)?;
                    let layer = entity(&args.element)?;
                    project.snapshot().text_layer(layer)?;
                    let category = booharu_context::LetteringCategory::parse(&args.category)
                        .context("invalid lettering category")?;
                    anyhow::ensure!(
                        args.confidence
                            .is_none_or(|value| value.is_finite() && (0.0..=1.0).contains(&value)),
                        "invalid label confidence"
                    );
                    let changed = project.update_context(|context| {
                        context.set_model_category(layer, category, args.confidence)
                    })?;
                    if changed {
                        changed_layers.push(layer);
                    }
                    json!({"changed":changed})
                }
                "flag_line" => {
                    let args: FlagLine = arguments(call)?;
                    let layer = entity(&args.element)?;
                    let snapshot = project.snapshot();
                    snapshot.text_layer(layer)?;
                    let mut page_number = None;
                    for (index, page) in snapshot.pages().enumerate() {
                        if let Some(group) = page.text_group()?
                            && group.text_layers()?.any(|item| item.id() == layer)
                        {
                            page_number = Some(index as u32 + 1);
                            break;
                        }
                    }
                    let page = page_number.context("text layer has no page")?;
                    project.update_context(|context| {
                        let flag = booharu_context::Flag::Review {
                            message: args.message,
                            page,
                        };
                        let line = context.lines.entry(layer).or_default();
                        if !line.flags.contains(&flag) {
                            line.flags.push(flag);
                        }
                    })?;
                    json!({"flagged":layer,"page":page})
                }
                _ => unreachable!(),
            };
            let commit = super::super::lettering::labels_changed(project, &changed_layers).await?;
            if let Some(commit) = &commit {
                project.record_commit(commit);
            }
            (commit, project.active_page(), value)
        };
        if let Some(commit) = commit {
            self.synchronize(commit, page).await?;
        }
        Invocation::changed(value)
    }

    async fn run_pipeline(&self, arguments: RunPipeline, control: &Control) -> Result<Invocation> {
        let scope = arguments.scope()?;
        let operation = arguments.operation.into();
        let translation_hints = {
            let projects = self.handle.state::<CurrentProject>();
            let project = projects.project.lock().await;
            let project = project.as_ref().context("no project is open")?;
            let snapshot = project.snapshot();
            let hints =
                super::super::context::translation_hints(&project.load_context()?, &snapshot)?;
            std::sync::Arc::new(hints)
        };
        let job = JobId::new();
        let stop = StopToken::default();
        {
            let processing = self.handle.state::<Processing>();
            let mut stops = processing.stops.lock();
            anyhow::ensure!(
                !self
                    .handle
                    .state::<super::super::runs::BatchState>()
                    .active(),
                "a chapter batch is running"
            );
            if !stops.is_empty() {
                bail!("another pipeline process is already running");
            }
            stops.insert(job, stop.clone());
        }

        let watcher = tauri::async_runtime::spawn({
            let control = control.clone();
            let stop = stop.clone();
            async move {
                control.cancelled().await;
                stop.stop();
            }
        });
        let mut committer = AgentCommitter { host: self.clone() };
        let request = koharu_pipeline::Request {
            operation,
            scope,
            stop,
            progress: None,
            inpainting_mask: None,
            translation_hints,
            redetect: false,
            policy: super::super::runs::RunSettings::load()?.policy(),
        };
        let mut request = request;
        let result = async {
            if super::super::context::operation_translates(&request.operation) {
                super::super::context::auto_read(&self.handle, &request.scope, &request.stop, job)
                    .await?;
            }
            let snapshot = {
                let projects = self.handle.state::<CurrentProject>();
                let projects = projects.project.lock().await;
                let project = projects.as_ref().context("no project is open")?;
                let snapshot = project.snapshot();
                request.translation_hints = std::sync::Arc::new(
                    super::super::context::translation_hints(&project.load_context()?, &snapshot)?,
                );
                snapshot
            };
            self.handle
                .state::<koharu_pipeline::Pipeline>()
                .execute(snapshot, request, &mut committer)
                .await
                .map_err(anyhow::Error::from)
        }
        .await;
        watcher.abort();
        self.handle.state::<Processing>().stops.lock().remove(&job);
        let report = result.map_err(|error| anyhow!(error))?;
        if report.status == RunStatus::Stopped {
            bail!("pipeline processing was cancelled");
        }
        Invocation::changed(json!({
            "base_revision": report.base,
            "final_revision": report.final_revision,
            "completed": report.completed,
            "total": report.total,
            "elapsed_ms": report.elapsed.as_millis(),
            "failures": report.failures,
        }))
    }
}

#[async_trait]
impl Host for KoharuHost {
    async fn context(&self) -> Result<Value> {
        self.project_context().await
    }

    fn tools(&self) -> Vec<Tool> {
        static TOOLS: OnceLock<Vec<Tool>> = OnceLock::new();
        TOOLS
            .get_or_init(|| {
                vec![
                    definition::<InspectProject>(
                        "inspect_project",
                        "Read the latest complete semantic project state after edits. This does not include page images.",
                    ),
                    definition::<ViewPage>(
                        "view_page",
                        "Render and inspect one page image. Call this only for pages whose visual appearance matters to the request.",
                    ),
                    definition::<RenamePage>("rename_page", "Rename a project page."),
                    definition::<MovePage>("move_page", "Move a page to a zero-based project index."),
                    definition::<DeletePages>("delete_pages", "Delete pages and their contents."),
                    definition::<AddTextBox>("add_text_box", "Add a paragraph text box to a page."),
                    definition::<SetText>("set_source_text", "Replace an element's source text."),
                    definition::<SetTranslation>(
                        "set_translation",
                        "Set an element's translation, or pass null to remove it.",
                    ),
                    definition::<SetTypography>(
                        "set_typography",
                        "Replace an element's typography settings. Font is a family name from available_fonts.",
                    ),
                    text_style_definition(),
                    definition::<AddGlossaryEntry>("add_glossary_entry", "Suggest a glossary term for this chapter without replacing an existing series term."),
                    definition::<SetLineCategory>("set_line_category", "Set a model lettering category for a text layer. A user label always wins."),
                    definition::<FlagLine>("flag_line", "Mark a text layer for human review with a short message."),
                    definition::<SetGeometry>(
                        "set_geometry",
                        "Replace an element's page-space polygon geometry.",
                    ),
                    definition::<SetVisibility>(
                        "set_visibility",
                        "Change visibility and/or opacity for elements.",
                    ),
                    definition::<DeleteElements>("delete_elements", "Delete elements and descendants."),
                    definition::<MoveElement>(
                        "move_element",
                        "Move an element under a page or element at a zero-based index.",
                    ),
                    definition::<RunPipeline>(
                        "run_pipeline",
                        "Run Koharu's configured processing pipeline for the whole project, selected pages, or selected text elements.",
                    ),
                    // WO-20 (one box per bubble) tool definitions go here.
                    definition::<crate::commands::boxes::FindSplitBubblesArgs>(
                        "find_split_bubbles",
                        "Find bubbles with multiple text boxes, including box IDs, source text, and translations. Pass a page ID to inspect one page; omit it to inspect the project. Report split bubbles to the user before joining unless the user asked to fix them.",
                    ),
                    definition::<crate::commands::boxes::JoinBubbleBoxesArgs>(
                        "join_bubble_boxes",
                        "Join all boxes in the selected text layer's bubble in reading order, preserving their text in one undoable edit. Report the split bubbles first unless the user asked to fix them.",
                    ),
                ]
            })
            .clone()
    }

    async fn invoke(&self, call: ToolCall, control: &Control) -> Result<Invocation> {
        match call.name.as_str() {
            "inspect_project" => {
                let _: InspectProject = arguments(&call)?;
                Invocation::read(self.project_context().await?)
            }
            "view_page" => {
                let arguments: ViewPage = arguments(&call)?;
                let page = entity(&arguments.page)?;
                let (label, snapshot) = {
                    let current = self.handle.state::<CurrentProject>();
                    let current = current.project.lock().await;
                    let project = current.as_ref().context("no project is open")?;
                    let snapshot = project.snapshot();
                    let label = snapshot.page(page)?.page()?.label;
                    (label, snapshot)
                };
                let desktop = self.handle.state::<Desktop>();
                let renderer = desktop.renderer();
                let rasterizer = desktop.rasterizer().await?;
                let bytes =
                    output::rendered_preview(&renderer, rasterizer, &snapshot, page).await?;
                Ok(
                    Invocation::read(json!({ "page": page, "label": label }))?.with_image(
                        format!("Rendered page {label} ({page})"),
                        format!("data:image/webp;base64,{}", STANDARD.encode(bytes)),
                    ),
                )
            }
            "rename_page" => {
                let arguments: RenamePage = arguments(&call)?;
                let page = entity(&arguments.page)?;
                self.mutate(|project| {
                    Box::pin(async move {
                        Ok((
                            project.rename_page(page, arguments.label).await?,
                            json!({ "page": page }),
                        ))
                    })
                })
                .await
            }
            "move_page" => {
                let arguments: MovePage = arguments(&call)?;
                let page = entity(&arguments.page)?;
                self.mutate(|project| {
                    Box::pin(async move {
                        Ok((
                            project.move_page(page, arguments.index).await?,
                            json!({ "page": page }),
                        ))
                    })
                })
                .await
            }
            "delete_pages" => {
                let arguments: DeletePages = arguments(&call)?;
                let pages = entities(&arguments.pages)?;
                self.mutate(|project| {
                    Box::pin(async move {
                        Ok((
                            project.delete_pages(pages.clone()).await?,
                            json!({ "pages": pages }),
                        ))
                    })
                })
                .await
            }
            "add_text_box" => {
                let arguments: AddTextBox = arguments(&call)?;
                let page = entity(&arguments.page)?;
                let frame = Frame {
                    x: arguments.x,
                    y: arguments.y,
                    width: arguments.width,
                    height: arguments.height,
                    angle_degrees: arguments.angle_degrees,
                };
                self.mutate(|project| {
                    Box::pin(async move {
                        let (commit, element) = project.add_text_box(page, frame).await?;
                        Ok((commit, json!({ "page": page, "element": element })))
                    })
                })
                .await
            }
            "set_source_text" => {
                let arguments: SetText = arguments(&call)?;
                let element = entity(&arguments.element)?;
                self.mutate(|project| {
                    Box::pin(async move {
                        Ok((
                            project.set_source_text(element, arguments.text).await?,
                            json!({ "element": element }),
                        ))
                    })
                })
                .await
            }
            "set_translation" => {
                let arguments: SetTranslation = arguments(&call)?;
                let element = entity(&arguments.element)?;
                self.mutate(|project| {
                    Box::pin(async move {
                        Ok((
                            project.set_translation(element, arguments.text).await?,
                            json!({ "element": element }),
                        ))
                    })
                })
                .await
            }
            "set_typography" => {
                let arguments: SetTypography = arguments(&call)?;
                let element = entity(&arguments.element)?;
                let typography = Typography {
                    preferred_font: arguments.preferred_font,
                    font_weight: arguments.font_weight,
                    font_style: arguments.font_style.map(Into::into),
                    size: arguments.size,
                    auto_fit: arguments.auto_fit,
                    color: arguments.color,
                    stroke_color: arguments.stroke_color,
                    stroke_width: arguments.stroke_width,
                    stroke_method: arguments.stroke_method.map(|value| match value {
                        AgentStrokeMethod::Expansion => koharu_scene::StrokeMethod::Expansion,
                        AgentStrokeMethod::Contour => koharu_scene::StrokeMethod::Contour,
                    }),
                    stroke_position: arguments.stroke_position.map(|value| match value {
                        AgentStrokePosition::Outside => koharu_scene::StrokePosition::Outside,
                        AgentStrokePosition::Center => koharu_scene::StrokePosition::Center,
                        AgentStrokePosition::Inside => koharu_scene::StrokePosition::Inside,
                    }),
                    stroke_join: arguments.stroke_join.map(|value| match value {
                        AgentStrokeJoin::Round => koharu_scene::StrokeJoin::Round,
                        AgentStrokeJoin::Miter => koharu_scene::StrokeJoin::Miter,
                        AgentStrokeJoin::Bevel => koharu_scene::StrokeJoin::Bevel,
                    }),
                    outer_stroke_color: arguments.outer_stroke_color,
                    outer_stroke_width: arguments.outer_stroke_width,
                    line_spacing: arguments.line_spacing,
                    letter_spacing: arguments.letter_spacing,
                    fill_gradient: arguments.fill_gradient,
                    alignment: arguments.alignment.map(Into::into),
                    writing_mode: arguments.writing_mode.map(Into::into),
                };
                self.mutate(|project| {
                    Box::pin(async move {
                        Ok((
                            project
                                .set_typography(vec![TypographyUpdate {
                                    layer: element,
                                    typography,
                                }])
                                .await?,
                            json!({ "element": element }),
                        ))
                    })
                })
                .await
            }
            "apply_text_style" => {
                let arguments: ApplyTextStyle = arguments(&call)?;
                let styles = crate::commands::styles::load_text_styles()?.read()?.clone();
                let detect_colors = koharu_pipeline::PipelineConfig::load()?
                    .read()?
                    .detect_text_color;
                self.mutate_optional(|project| {
                    Box::pin(async move {
                        apply_agent_text_style(project, arguments, &styles, detect_colors).await
                    })
                })
                .await
            }
            "set_geometry" => {
                let arguments: SetGeometry = arguments(&call)?;
                let element = entity(&arguments.element)?;
                let points = arguments
                    .points
                    .into_iter()
                    .map(|point| Point {
                        x: point.x,
                        y: point.y,
                    })
                    .collect();
                self.mutate(|project| {
                    Box::pin(async move {
                        Ok((
                            project
                                .set_geometry(vec![GeometryUpdate {
                                    layer: element,
                                    points: Some(points),
                                }])
                                .await?,
                            json!({ "element": element }),
                        ))
                    })
                })
                .await
            }
            "set_visibility" => {
                let arguments: SetVisibility = arguments(&call)?;
                let elements = entities(&arguments.elements)?;
                self.mutate(|project| {
                    Box::pin(async move {
                        Ok((
                            project
                                .set_visibility(
                                    elements.clone(),
                                    arguments.visible,
                                    arguments.opacity,
                                )
                                .await?,
                            json!({ "elements": elements }),
                        ))
                    })
                })
                .await
            }
            "delete_elements" => {
                let arguments: DeleteElements = arguments(&call)?;
                let elements = entities(&arguments.elements)?;
                self.mutate(|project| {
                    Box::pin(async move {
                        Ok((
                            project.delete_layers(elements.clone()).await?,
                            json!({ "elements": elements }),
                        ))
                    })
                })
                .await
            }
            "move_element" => {
                let arguments: MoveElement = arguments(&call)?;
                let element = entity(&arguments.element)?;
                let parent = entity(&arguments.parent)?;
                self.mutate(|project| {
                    Box::pin(async move {
                        Ok((
                            project.move_layer(element, parent, arguments.index).await?,
                            json!({ "element": element, "parent": parent }),
                        ))
                    })
                })
                .await
            }
            "add_glossary_entry" | "set_line_category" | "flag_line" => {
                self.context_tool(&call).await
            }
            "run_pipeline" => self.run_pipeline(arguments(&call)?, control).await,
            // WO-20 (one box per bubble) tool calls go here.
            "find_split_bubbles" => {
                let arguments: crate::commands::boxes::FindSplitBubblesArgs = arguments(&call)?;
                let page = arguments.page.as_deref().map(entity).transpose()?;
                let current = self.handle.state::<CurrentProject>();
                let current = current.project.lock().await;
                let project = current.as_ref().context("no project is open")?;
                Invocation::read(project.find_split_bubbles(page)?)
            }
            "join_bubble_boxes" => {
                let arguments: crate::commands::boxes::JoinBubbleBoxesArgs = arguments(&call)?;
                let layer = entity(&arguments.layer)?;
                self.mutate_optional(|project| {
                    Box::pin(async move {
                        let (commit, kept) = project.join_bubble_boxes(layer).await?;
                        Ok((commit, json!({ "layer": kept })))
                    })
                })
                .await
            }
            name => bail!("unknown Koharu tool {name}"),
        }
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

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ApplyTextStyle {
    elements: Vec<String>,
    /// One of rectangle, round, spiky, none. Applies the default and the shape's style.
    shape: String,
}

fn text_style_definition() -> Tool {
    let mut tool = definition::<ApplyTextStyle>(
        "apply_text_style",
        "Restyle text elements as if their bubble had another shape. Use after view_page shows that a bubble was sorted wrongly, for example a shout in a bubble detected as round.",
    );
    tool.parameters["properties"]["shape"] =
        json!({"type": "string", "enum": ["rectangle", "round", "spiky", "none"]});
    tool
}

async fn apply_agent_text_style(
    project: &mut Project,
    arguments: ApplyTextStyle,
    styles: &koharu_renderer::TextStyles,
    detect_colors: bool,
) -> Result<(Option<Commit>, crate::commands::styles::StyleApplyResult)> {
    use crate::commands::styles::StyleScope;
    let scope = StyleScope::Layers(entities(&arguments.elements)?);
    let shape_style = match arguments.shape.as_str() {
        "rectangle" => &styles.shape_styles.rectangle,
        "round" => &styles.shape_styles.round,
        "spiky" => &styles.shape_styles.spiky,
        "none" => &styles.shape_styles.free,
        shape => bail!("unknown shape {shape}; valid shapes: rectangle, round, spiky, none"),
    };
    styles.validate()?;
    project
        .apply_style(scope, true, true, |_, _, typography| {
            styles
                .default_style
                .apply_to_detected(typography, detect_colors);
            shape_style.apply_to_detected(typography, detect_colors);
            Ok(())
        })
        .await
}

struct AgentCommitter {
    host: KoharuHost,
}

#[async_trait]
impl Committer for AgentCommitter {
    async fn commit(&mut self, output: StageOutput) -> Result<Snapshot> {
        let (commit, page) = {
            let current = self.host.handle.state::<CurrentProject>();
            let mut current = current.project.lock().await;
            let project = current.as_mut().context("no project is open")?;
            let Some(commit) = project.commit_rebased(output.patch).await? else {
                return Ok(project.snapshot());
            };
            project.record_commit(&commit);
            if output.stage == Stage::Translation {
                super::super::context::refresh_flags(project)?;
            }
            (commit, project.active_page())
        };
        let snapshot = commit.snapshot.clone();
        self.host.synchronize(commit, page).await?;
        Ok(snapshot)
    }
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

#[derive(Deserialize, JsonSchema)]
struct InspectProject {}

#[derive(Deserialize, JsonSchema)]
struct ViewPage {
    page: String,
}

#[derive(Deserialize, JsonSchema)]
struct RenamePage {
    page: String,
    label: String,
}

#[derive(Deserialize, JsonSchema)]
struct MovePage {
    page: String,
    index: usize,
}

#[derive(Deserialize, JsonSchema)]
struct DeletePages {
    pages: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
struct AddTextBox {
    page: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    #[serde(default)]
    angle_degrees: f32,
}

#[derive(Deserialize, JsonSchema)]
struct SetText {
    element: String,
    text: String,
}

#[derive(Deserialize, JsonSchema)]
struct SetTranslation {
    element: String,
    text: Option<String>,
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentTextAlignment {
    Start,
    Center,
    End,
    Justify,
}

impl From<AgentTextAlignment> for koharu_scene::TextAlignment {
    fn from(value: AgentTextAlignment) -> Self {
        match value {
            AgentTextAlignment::Start => Self::Start,
            AgentTextAlignment::Center => Self::Center,
            AgentTextAlignment::End => Self::End,
            AgentTextAlignment::Justify => Self::Justify,
        }
    }
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentWritingMode {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentFontStyle {
    Normal,
    Italic,
    Oblique,
}

impl From<AgentFontStyle> for koharu_scene::FontStyle {
    fn from(value: AgentFontStyle) -> Self {
        match value {
            AgentFontStyle::Normal => Self::Normal,
            AgentFontStyle::Italic => Self::Italic,
            AgentFontStyle::Oblique => Self::Oblique,
        }
    }
}

impl From<AgentWritingMode> for koharu_scene::WritingMode {
    fn from(value: AgentWritingMode) -> Self {
        match value {
            AgentWritingMode::Horizontal => Self::Horizontal,
            AgentWritingMode::Vertical => Self::Vertical,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentStrokeMethod {
    Expansion,
    Contour,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentStrokePosition {
    Outside,
    Center,
    Inside,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentStrokeJoin {
    Round,
    Miter,
    Bevel,
}

#[derive(Deserialize, JsonSchema)]
struct SetTypography {
    element: String,
    preferred_font: Option<String>,
    font_weight: Option<u16>,
    font_style: Option<AgentFontStyle>,
    size: Option<f32>,
    auto_fit: bool,
    color: Option<[u8; 4]>,
    stroke_color: Option<[u8; 4]>,
    stroke_width: Option<f32>,
    stroke_method: Option<AgentStrokeMethod>,
    stroke_position: Option<AgentStrokePosition>,
    stroke_join: Option<AgentStrokeJoin>,
    /// Color of a second outline outside the first.
    outer_stroke_color: Option<[u8; 4]>,
    /// Width of the second outline in pixels; omit or 0 for none.
    outer_stroke_width: Option<f32>,
    line_spacing: Option<f32>,
    letter_spacing: Option<f32>,
    #[schemars(with = "Option<AgentFillGradient>")]
    fill_gradient: Option<koharu_scene::FillGradient>,
    alignment: Option<AgentTextAlignment>,
    writing_mode: Option<AgentWritingMode>,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct AgentFillGradient {
    kind: AgentGradientKind,
    angle_degrees: f32,
    stops: Vec<AgentGradientStop>,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
enum AgentGradientKind {
    Linear,
    Radial,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct AgentGradientStop {
    offset: f32,
    color: [u8; 4],
}

#[derive(Deserialize, JsonSchema)]
struct SetGeometry {
    element: String,
    points: Vec<AgentPoint>,
}

#[derive(Deserialize, JsonSchema)]
struct AgentPoint {
    x: f64,
    y: f64,
}

#[derive(Deserialize, JsonSchema)]
struct SetVisibility {
    elements: Vec<String>,
    visible: Option<bool>,
    opacity: Option<f32>,
}

#[derive(Deserialize, JsonSchema)]
struct DeleteElements {
    elements: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
struct MoveElement {
    element: String,
    parent: String,
    index: usize,
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentPipelineOperation {
    Full,
    Detection,
    Ocr,
    Translation,
    Inpainting,
}

impl From<AgentPipelineOperation> for Operation {
    fn from(value: AgentPipelineOperation) -> Self {
        match value {
            AgentPipelineOperation::Full => Self::Full,
            AgentPipelineOperation::Detection => Self::Only {
                stage: Stage::Detection,
            },
            AgentPipelineOperation::Ocr => Self::Only { stage: Stage::Ocr },
            AgentPipelineOperation::Translation => Self::Only {
                stage: Stage::Translation,
            },
            AgentPipelineOperation::Inpainting => Self::Only {
                stage: Stage::Inpainting,
            },
        }
    }
}

#[derive(Deserialize, JsonSchema)]
struct RunPipeline {
    operation: AgentPipelineOperation,
    #[serde(default)]
    pages: Vec<String>,
    #[serde(default)]
    elements: Vec<String>,
}

impl RunPipeline {
    fn scope(&self) -> Result<Scope> {
        match (self.pages.is_empty(), self.elements.is_empty()) {
            (true, true) => Ok(Scope::Project),
            (false, true) => Ok(Scope::Pages(entities(&self.pages)?)),
            (true, false) => Ok(Scope::Entities(entities(&self.elements)?)),
            (false, false) => bail!("pipeline scope cannot contain both pages and elements"),
        }
    }
}

#[cfg(test)]
mod text_style_tests {
    use super::*;
    use koharu_renderer::{FontPart, ShapeStyles, TextStyle, TextStyles};
    use koharu_scene::{
        At, BubbleRegion, FlowsIn, Generation, Geometry, Origin, PageDraft, Point, ProducerId,
        TextLayout, TextLayoutKind,
    };

    #[tokio::test]
    async fn context_and_forced_shape_style_share_the_real_batch_and_undo() {
        let directory = tempfile::tempdir().unwrap();
        let mut project =
            Project::create("agent styles".into(), directory.path().join("project.khr"))
                .await
                .unwrap();
        let generation = Generation::new(ProducerId::new("dev.koharu.pipeline.detection").unwrap());
        let mut edit = project.snapshot().edit_as(generation.clone());
        let page = edit
            .add_page(PageDraft::new("Page 3", 300.0, 300.0), At::End)
            .unwrap();
        let points = (0..24)
            .map(|index| {
                let radius = if index % 2 == 0 { 100.0 } else { 60.0 };
                let angle = index as f64 * std::f64::consts::TAU / 24.0;
                Point {
                    x: 150.0 + radius * angle.cos(),
                    y: 150.0 + radius * angle.sin(),
                }
            })
            .collect();
        let bubble = edit
            .add_analysis_region::<BubbleRegion>(
                page,
                At::End,
                &Geometry {
                    origin: Origin::Generated(generation.clone()),
                    points,
                },
                None,
            )
            .unwrap();
        let mut layers = Vec::new();
        for _ in 0..2 {
            let content = edit.add_text_content(page, At::End).unwrap();
            let layer = edit
                .add_text_layer(
                    page,
                    At::End,
                    content,
                    &TextLayout {
                        origin: Origin::Generated(generation.clone()),
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: None,
                    },
                )
                .unwrap();
            edit.set(
                layer,
                &koharu_scene::Typography {
                    origin: Origin::Generated(generation.clone()),
                    preferred_font: Some("Original".into()),
                    font_weight: None,
                    font_style: None,
                    size: None,
                    auto_fit: true,
                    color: Some([1, 2, 3, 255]),
                    stroke_color: None,
                    stroke_width: None,
                    stroke_method: None,
                    stroke_position: None,
                    stroke_join: None,
                    outer_stroke_color: None,
                    outer_stroke_width: None,
                    line_spacing: None,
                    letter_spacing: None,
                    fill_gradient: None,
                    alignment: None,
                    writing_mode: Some(koharu_scene::WritingMode::Vertical),
                    extensions: Default::default(),
                },
            )
            .unwrap();
            edit.relate::<FlowsIn>(layer, bubble).unwrap();
            layers.push(layer);
        }
        project
            .commit_rebased(edit.finish().unwrap())
            .await
            .unwrap()
            .unwrap();
        let original = project
            .snapshot()
            .component::<koharu_scene::Typography>(layers[0])
            .unwrap()
            .unwrap();
        let first_text = |pages: &[Value]| {
            pages[0]["layers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|layer| layer["type"] == "text")
                .unwrap()
                .clone()
        };
        let pages = agent_pages(&project.snapshot()).unwrap();
        assert_eq!(first_text(&pages)["bubble_shape"], "spiky");
        assert_eq!(first_text(&pages)["style_origin"], "generated");
        assert!(
            first_text(&pages)["shape_measures"]["spikes"]
                .as_u64()
                .unwrap()
                >= 5
        );
        let styles = TextStyles {
            shape_styles: ShapeStyles {
                spiky: TextStyle {
                    font: Some(FontPart {
                        preferred_font: Some("Spiky Font".into()),
                        font_weight: None,
                        font_style: None,
                    }),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let configuration = text_styles_context(&styles);
        assert_eq!(
            configuration["shape_styles"]["spiky"]["font"]["preferred_font"],
            "Spiky Font"
        );
        assert!(configuration.get("presets").is_none());
        let args = || ApplyTextStyle {
            elements: layers.iter().map(ToString::to_string).collect(),
            shape: "spiky".into(),
        };
        let (commit, result) = apply_agent_text_style(&mut project, args(), &styles, true)
            .await
            .unwrap();
        assert_eq!((result.changed, result.skipped), (2, 0));
        project.record_commit(&commit.unwrap());
        assert_eq!(project.undo.len(), 1);
        for layer in &layers {
            let typography = project
                .snapshot()
                .component::<koharu_scene::Typography>(*layer)
                .unwrap()
                .unwrap();
            assert_eq!(typography.preferred_font.as_deref(), Some("Spiky Font"));
            assert_eq!(typography.origin, Origin::User);
            assert_eq!(typography.color, original.color);
            assert_eq!(typography.writing_mode, original.writing_mode);
        }
        assert_eq!(
            first_text(&agent_pages(&project.snapshot()).unwrap())["style_origin"],
            "user"
        );
        let (commit, result) = apply_agent_text_style(&mut project, args(), &styles, true)
            .await
            .unwrap();
        assert!(commit.is_none());
        assert_eq!((result.changed, result.skipped), (0, 0));
        assert!(!Invocation::read(result).unwrap().changed);
        project.undo().await.unwrap();
        for layer in &layers {
            assert_eq!(
                project
                    .snapshot()
                    .component::<koharu_scene::Typography>(*layer)
                    .unwrap()
                    .unwrap(),
                original
            );
        }
        let mut invalid = args();
        invalid.shape = "triangle".into();
        let error = apply_agent_text_style(&mut project, invalid, &styles, true)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("rectangle, round, spiky, none"));
    }
}

#[derive(Deserialize, JsonSchema)]
struct AddGlossaryEntry {
    source: String,
    target: String,
    kind: String,
    note: String,
}
#[derive(Deserialize, JsonSchema)]
struct SetLineCategory {
    element: String,
    category: String,
    confidence: Option<f32>,
}
#[derive(Deserialize, JsonSchema)]
struct FlagLine {
    element: String,
    message: String,
}
