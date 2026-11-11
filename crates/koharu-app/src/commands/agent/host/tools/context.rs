//! Chapter context: glossary suggestions, lettering categories and review flags.

use super::*;

pub(super) fn tools() -> Vec<Tool> {
    vec![
            definition::<AddGlossaryEntry>("add_glossary_entry", "Suggest a glossary term for this chapter without replacing an existing series term."),
            definition::<SetLineCategory>("set_line_category", "Set a model lettering category for a text layer. A user label always wins."),
            definition::<FlagLine>("flag_line", "Mark a text layer for human review with a short message."),
    ]
}

pub(super) async fn invoke(
    host: &KoharuHost,
    call: &ToolCall,
    _control: &Control,
) -> Result<Option<Invocation>> {
    let result: Result<Invocation> = match call.name.as_str() {
        "add_glossary_entry" | "set_line_category" | "flag_line" => {
            host.context_tool(call).await
        }
        _ => return Ok(None),
    };
    result.map(Some)
}

impl KoharuHost {
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
                    let series = crate::commands::context::linked_series(&context)?;
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
            let commit = crate::commands::lettering::labels_changed(project, &changed_layers).await?;
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
