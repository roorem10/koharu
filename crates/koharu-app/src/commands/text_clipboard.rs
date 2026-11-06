//! Internal text-box clipboard. Idea: NeKoharu 98bb6f0e (nekojin116).

#[cfg(test)]
mod tests;

use std::collections::HashSet;

use anyhow::{Context as _, ensure};
use koharu_desktop::Desktop;
use koharu_renderer::{LayerKind, Renderer};
use koharu_scene::{
    At, Authored, Commit, EntityId, Geometry, ProjectId, Revision, Snapshot, SourceText, TextLayout,
    TextRole, TextRuns, Translation, Typography, Visibility,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use tokio::sync::Mutex;

use super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    project::{CurrentProject, Project},
};

#[derive(Default)]
pub(crate) struct TextClipboard {
    value: Mutex<Option<Clipboard>>,
}

struct Clipboard {
    boxes: TextBoxes,
    pastes: u32,
}

#[derive(Clone, Debug, Serialize, Type)]
pub(crate) struct TextBoxesCommit {
    pub revision: Revision,
    pub layers: Vec<EntityId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TextBoxes {
    project: ProjectId,
    boxes: Vec<TextBox>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TextBox {
    page: EntityId,
    source: SourceText,
    translation: Translation,
    role: Option<TextRole>,
    layout: TextLayout,
    geometry: Option<Geometry>,
    typography: Option<Typography>,
    runs: Option<TextRuns>,
    visibility: Option<Visibility>,
}

impl TextBoxes {
    async fn capture(
        snapshot: &Snapshot,
        layers: &[EntityId],
        renderer: &Renderer,
    ) -> anyhow::Result<Self> {
        ensure!(!layers.is_empty(), "select at least one text layer");
        let selected = layers.iter().copied().collect::<HashSet<_>>();
        for &layer in &selected {
            snapshot.text_layer(layer)?;
        }
        let mut boxes = Vec::with_capacity(selected.len());
        // Scene order, rather than selection order, preserves the stacking of overlaps.
        for page in snapshot.pages() {
            let mut rendered = None;
            for layer in page.text_layers()? {
                if !selected.contains(&layer.id()) {
                    continue;
                }
                let content = layer.content()?;
                let mut layout = layer.layout()?;
                let mut geometry = layer.frame()?;
                if snapshot.component::<Geometry>(layer.id())?.is_none()
                    && layer.balloon_target()?.is_some()
                {
                    // Shared bubbles divide their contour between text flows. Freeze the
                    // renderer's individual placement, not the entire bubble's frame.
                    if rendered.is_none() {
                        rendered = Some(renderer.render(snapshot, page.id()).await?);
                    }
                    if let Some(prepared) =
                        rendered.as_ref().and_then(|frame| frame.layer(layer.id()))
                    {
                        geometry = Some(prepared.geometry().clone());
                        if let LayerKind::Text(text) = prepared.kind() {
                            layout.angle_degrees = Some(text.angle_degrees);
                        }
                    }
                }
                boxes.push(TextBox {
                    page: page.id(),
                    // Empty user-owned records also protect untranslated and blank boxes
                    // from later OCR and translation runs.
                    source: content.source()?.unwrap_or_else(|| SourceText {
                        text: Authored::user(String::new()),
                        language: None,
                    }),
                    translation: content.translation()?.unwrap_or_else(|| Translation {
                        text: Authored::user(String::new()),
                        language: None,
                    }),
                    role: content.role()?,
                    layout,
                    // Freeze automatic placement before dropping its analysis relations.
                    geometry,
                    typography: layer.typography()?,
                    runs: snapshot.component::<TextRuns>(layer.id())?,
                    visibility: layer.visibility()?,
                });
            }
        }
        ensure!(boxes.len() == selected.len(), "text layer is not on a page");
        Ok(Self {
            project: snapshot.project_id(),
            boxes,
        })
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_text_clipboard_available(
    clipboard: State<'_, TextClipboard>,
) -> Result<bool, Error> {
    Ok(clipboard.value.lock().await.is_some())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn copy_text_boxes(
    layers: Vec<EntityId>,
    clipboard: State<'_, TextClipboard>,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
) -> Result<(), Error> {
    let mut clipboard = clipboard.value.lock().await;
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    let boxes = TextBoxes::capture(&project.snapshot(), &layers, &desktop.renderer()).await?;
    *clipboard = Some(Clipboard { boxes, pastes: 0 });
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn cut_text_boxes(
    layers: Vec<EntityId>,
    clipboard: State<'_, TextClipboard>,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<(), Error> {
    let (commit, page) = {
        let mut clipboard = clipboard.value.lock().await;
        let mut projects = project.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let boxes = TextBoxes::capture(&project.snapshot(), &layers, &desktop.renderer()).await?;
        let commit = project.delete_layers(layers).await?;
        project.record_commit(&commit);
        *clipboard = Some(Clipboard { boxes, pastes: 0 });
        (commit, project.active_page())
    };
    desktop.synchronize(&commit.snapshot, page, &commit).await?;
    canvas_channel.channel.publish(desktop.canvas_state());
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn paste_text_boxes(
    page: EntityId,
    clipboard: State<'_, TextClipboard>,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<TextBoxesCommit, Error> {
    let (commit, active_page, layers) = {
        let mut clipboard = clipboard.value.lock().await;
        let clipboard = clipboard
            .as_mut()
            .context("the text-box clipboard is empty")?;
        let mut projects = project.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let next_paste = clipboard.pastes.saturating_add(1);
        let (commit, layers) = project
            .paste_text_boxes(&clipboard.boxes, page, f64::from(next_paste) * 10.0)
            .await?;
        project.record_commit(&commit);
        clipboard.pastes = next_paste;
        (commit, project.active_page(), layers)
    };
    desktop.synchronize(&commit.snapshot, active_page, &commit).await?;
    canvas_channel.channel.publish(desktop.canvas_state());
    Ok(TextBoxesCommit {
        revision: commit.revision,
        layers,
    })
}

impl Project {
    pub(crate) async fn copy_text_boxes(
        &mut self,
        layers: &[EntityId],
        page: EntityId,
        renderer: &Renderer,
    ) -> anyhow::Result<(Commit, Vec<EntityId>)> {
        let boxes = TextBoxes::capture(&self.snapshot(), layers, renderer).await?;
        self.paste_text_boxes(&boxes, page, 10.0).await
    }

    async fn paste_text_boxes(
        &mut self,
        boxes: &TextBoxes,
        page: EntityId,
        same_page_offset: f64,
    ) -> anyhow::Result<(Commit, Vec<EntityId>)> {
        let snapshot = self.snapshot();
        let size = snapshot.page(page)?.page()?;
        let mut layers = Vec::with_capacity(boxes.boxes.len());
        let patch = snapshot.patch(|edit| {
            for value in &boxes.boxes {
                let content = edit.add_text_content(page, At::End)?;
                // Ordinary scene edits stamp every copied component as user-owned.
                // Only Presents is recreated: no FitsTo, FlowsIn or RecognizedFrom
                // relation may tie this independent lettering to its source bubble.
                edit.set(content, &value.source)?;
                edit.set(content, &value.translation)?;
                if let Some(role) = &value.role {
                    edit.set(content, role)?;
                }
                let layer = edit.add_text_layer(page, At::End, content, &value.layout)?;
                if let Some(geometry) = &value.geometry {
                    let offset = if boxes.project == snapshot.project_id() && value.page == page {
                        same_page_offset
                    } else {
                        0.0
                    };
                    edit.set(
                        layer,
                        &clamp_geometry(geometry, size.width, size.height, offset),
                    )?;
                }
                if let Some(typography) = &value.typography {
                    edit.set(layer, typography)?;
                }
                if let Some(runs) = &value.runs {
                    edit.set(layer, runs)?;
                }
                if let Some(visibility) = &value.visibility {
                    edit.set(layer, visibility)?;
                }
                layers.push(layer);
            }
            Ok(())
        })?;
        Ok((self.commit(patch).await?, layers))
    }
}

fn clamp_geometry(geometry: &Geometry, width: f64, height: f64, offset: f64) -> Geometry {
    let mut result = geometry.clone();
    let bounds = geometry.points.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |[left, top, right, bottom], point| {
            [
                left.min(point.x),
                top.min(point.y),
                right.max(point.x),
                bottom.max(point.y),
            ]
        },
    );
    let [left, top, right, bottom] = bounds;
    // Move the entire polygon; never resize lettering or distort a rotated box.
    // A box larger than the destination starts at its top/left edge.
    let dx = (left + offset).clamp(0.0, (width - (right - left)).max(0.0)) - left;
    let dy = (top + offset).clamp(0.0, (height - (bottom - top)).max(0.0)) - top;
    for point in &mut result.points {
        point.x += dx;
        point.y += dy;
    }
    result
}
