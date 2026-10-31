use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result, ensure};
use koharu_desktop::Desktop;
use koharu_scene::{Authored, Commit, EntityId, ProjectId, Revision, Snapshot, Translation};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{State, WebviewWindow};
use tauri_runtime_cef::CefRuntime;

use super::super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    project::{CurrentProject, Project},
};

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct ScriptChange {
    pub layer: EntityId,
    pub page: u32,
    pub page_label: String,
    pub box_number: u32,
    pub old_text: String,
    pub new_text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct ScriptPreview {
    pub project: ProjectId,
    pub project_name: String,
    pub revision: Revision,
    pub changes: Vec<ScriptChange>,
    pub unchanged: u32,
    pub not_found: u32,
}

struct ScriptRow {
    layer: EntityId,
    source: String,
    translation: String,
}

struct ScriptPage {
    label: String,
    rows: Vec<ScriptRow>,
}

fn script_pages(snapshot: &Snapshot) -> Result<Vec<ScriptPage>> {
    snapshot
        .pages()
        .map(|page| {
            let rows = page
                .text_layers()?
                .map(|layer| {
                    let content = layer.content()?;
                    Ok(ScriptRow {
                        layer: layer.id(),
                        source: content.source()?.map(|s| s.text.value).unwrap_or_default(),
                        translation: content
                            .translation()?
                            .map(|t| t.text.value)
                            .unwrap_or_default(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(ScriptPage {
                label: page.page()?.label,
                rows,
            })
        })
        .collect()
}

fn write_script(name: &str, pages: &[ScriptPage]) -> Result<String> {
    let mut output = format!(
        "# Booharu script 1\n# Project: {}\n\n",
        name.replace(['\r', '\n'], " ")
    );
    for (page, data) in pages.iter().enumerate() {
        output.push_str(&format!(
            "=== Page {}: {} ===\n\n",
            page + 1,
            data.label.replace(['\r', '\n'], " ")
        ));
        for (number, row) in data.rows.iter().enumerate() {
            let translation = row.translation.replace("\r\n", "\n");
            ensure!(
                !translation.starts_with("> ")
                    && !translation
                        .lines()
                        .any(|line| line.starts_with("--- Box ") || line.starts_with("=== Page ")),
                "box {} contains reserved script syntax; edit this translation in the app",
                row.layer
            );
            output.push_str(&format!("--- Box {} {} ---\n", number + 1, row.layer));
            let source = row
                .source
                .replace("\r\n", "\n")
                .split('\n')
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            output.push_str(&source);
            if !translation.is_empty() {
                output.push('\n');
                output.push_str(&translation);
            }
            output.push_str("\n\n");
        }
    }
    Ok(output)
}

// UTF-8 script 1, with LF or CRLF: "# Booharu script 1", "# Project: <name>",
// a blank line, "=== Page <number>: <label> ===", a blank line, then blocks:
// "--- Box <number> <text layer UUID> ---", zero or more source lines prefixed
// "> ", translation verbatim, and one separator blank line. Source lines and
// header numbers/labels never write to the project; UUIDs are authoritative.
// Initial "> " lines and lines starting a Page/Box header are reserved syntax.
// Remove exactly the block separator, preserving translation blank lines.
fn parse_script(text: &str) -> Result<Vec<(EntityId, String)>> {
    let text = text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .replace("\r\n", "\n");
    let mut lines = text.split_inclusive('\n');
    ensure!(
        lines.next().map(|s| s.trim_end_matches('\n')) == Some("# Booharu script 1"),
        "invalid Booharu script header; expected # Booharu script 1"
    );
    ensure!(
        lines.next().is_some_and(|s| s.starts_with("# Project: ")),
        "missing script project header"
    );
    let mut offset = text.len() - lines.clone().map(str::len).sum::<usize>();
    let mut seen_page = false;
    let mut current = None;
    let mut output = Vec::new();
    let mut ids = BTreeSet::new();
    for raw in lines {
        let line = raw.trim_end_matches('\n');
        let page_header = line.starts_with("=== Page ");
        let box_header = line.starts_with("--- Box ");
        if page_header || box_header {
            if let Some((layer, start)) = current.take() {
                output.push((layer, translation_body(&text[start..offset])));
            }
            if page_header {
                let header = line
                    .strip_prefix("=== Page ")
                    .and_then(|s| s.strip_suffix(" ==="))
                    .context("broken script page header")?;
                let (number, _) = header
                    .split_once(": ")
                    .context("broken script page header")?;
                ensure!(
                    number.parse::<u32>().is_ok_and(|n| n > 0),
                    "invalid script page number"
                );
                seen_page = true;
            } else {
                ensure!(seen_page, "script box has no page header");
                let header = line
                    .strip_prefix("--- Box ")
                    .and_then(|s| s.strip_suffix(" ---"))
                    .context("broken script box header")?;
                let (number, id) = header.split_once(' ').context("broken script box header")?;
                ensure!(
                    number.parse::<u32>().is_ok_and(|n| n > 0),
                    "invalid script box number"
                );
                let layer: EntityId =
                    serde_json::from_value(serde_json::Value::String(id.to_owned()))
                        .context("invalid script text layer id")?;
                ensure!(ids.insert(layer), "duplicate script box id {layer}");
                current = Some((layer, offset + raw.len()));
            }
        } else if current.is_none() {
            ensure!(line.is_empty(), "unexpected text outside a script box");
        }
        offset += raw.len();
    }
    if let Some((layer, start)) = current {
        output.push((layer, translation_body(&text[start..])));
    }
    Ok(output)
}

fn translation_body(body: &str) -> String {
    let mut body = body
        .strip_suffix("\n\n")
        .unwrap_or_else(|| body.strip_suffix('\n').unwrap_or(body));
    while body.starts_with("> ") {
        body = body.split_once('\n').map_or("", |(_, rest)| rest);
    }
    body.to_owned()
}

fn preview_import(snapshot: &Snapshot, name: &str, text: &str) -> Result<ScriptPreview> {
    let rows = script_pages(snapshot)?
        .into_iter()
        .enumerate()
        .flat_map(|(page, data)| {
            data.rows.into_iter().enumerate().map(move |(number, row)| {
                (
                    row.layer,
                    (
                        page as u32 + 1,
                        data.label.clone(),
                        number as u32 + 1,
                        row.translation,
                    ),
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    let mut preview = ScriptPreview {
        project: snapshot.project_id(),
        project_name: name.to_owned(),
        revision: snapshot.revision(),
        changes: Vec::new(),
        unchanged: 0,
        not_found: 0,
    };
    for (layer, translation) in parse_script(text)? {
        let Some((page, label, number, old)) = rows.get(&layer) else {
            preview.not_found += 1;
            continue;
        };
        if old.replace("\r\n", "\n") == translation {
            preview.unchanged += 1;
        } else {
            preview.changes.push(ScriptChange {
                layer,
                page: *page,
                page_label: label.clone(),
                box_number: *number,
                old_text: old.clone(),
                new_text: translation,
            });
        }
    }
    Ok(preview)
}

impl Project {
    async fn apply_script(&mut self, preview: ScriptPreview) -> Result<Option<Commit>> {
        let snapshot = self.snapshot();
        ensure!(
            snapshot.project_id() == preview.project
                && self.name == preview.project_name
                && snapshot.revision() == preview.revision,
            "the project changed; import the script again to refresh the preview"
        );
        if preview.changes.is_empty() {
            return Ok(None);
        }
        let mut seen = BTreeSet::new();
        let updates = preview
            .changes
            .into_iter()
            .map(|change| {
                let content = snapshot.text_layer(change.layer)?.content()?;
                let old = content.translation()?;
                ensure!(
                    old.as_ref()
                        .map(|t| t.text.value.as_str())
                        .unwrap_or_default()
                        == change.old_text,
                    "a translation changed; refresh the script preview"
                );
                ensure!(
                    seen.insert(content.id()),
                    "several script boxes refer to the same text content"
                );
                Ok((
                    change.layer,
                    content.id(),
                    old.and_then(|t| t.language),
                    change.new_text,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let patch = snapshot.patch(|edit| {
            for (layer, content, language, text) in updates {
                edit.promote_entity_to_user(layer)?;
                edit.promote_entity_to_user(content)?;
                edit.set(
                    content,
                    &Translation {
                        text: Authored::user(text),
                        language,
                    },
                )?;
            }
            Ok(())
        })?;
        Ok(Some(self.commit(patch).await?))
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn export_script(
    window: WebviewWindow<CefRuntime>,
    project: State<'_, CurrentProject>,
) -> std::result::Result<(), Error> {
    let (name, snapshot) = {
        let project = project.project.lock().await;
        let project = project.as_ref().context("no project is open")?;
        (project.name.clone(), project.snapshot())
    };
    let text = write_script(&name, &script_pages(&snapshot)?)?;
    if let Some(file) = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .add_filter("TXT", &["txt"])
        .set_file_name(format!("{name}.txt"))
        .save_file()
        .await
    {
        tokio::fs::write(file.path(), text)
            .await
            .context("failed to export script")?;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn preview_script_import(
    window: WebviewWindow<CefRuntime>,
    project: State<'_, CurrentProject>,
) -> std::result::Result<Option<ScriptPreview>, Error> {
    let (snapshot, name) = {
        let project = project.project.lock().await;
        let project = project.as_ref().context("no project is open")?;
        (project.snapshot(), project.name.clone())
    };
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .add_filter("TXT", &["txt"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let text = tokio::fs::read_to_string(file.path())
        .await
        .context("script must be a readable UTF-8 text file")?;
    Ok(Some(preview_import(&snapshot, &name, &text)?))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn apply_script_import(
    preview: ScriptPreview,
    project: State<'_, CurrentProject>,
    desktop: State<'_, Desktop>,
    canvas_channel: State<'_, CanvasChannel>,
) -> std::result::Result<(), Error> {
    let (commit, page) = {
        let mut project = project.project.lock().await;
        let project = project.as_mut().context("no project is open")?;
        let commit = project.apply_script(preview).await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, project.active_page())
    };
    if let Some(commit) = commit {
        desktop.synchronize(&commit.snapshot, page, &commit).await?;
        canvas_channel.channel.publish(desktop.canvas_state());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
