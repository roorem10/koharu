//! Word styles and text edits share one scene patch, so undo restores both.

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use anyhow::{Context as _, ensure};
use koharu_desktop::Desktop;
use koharu_renderer::TextStyles;
use koharu_scene::{
    Authored, Commit, Component as _, ComponentOwner, Edit, EntityId, Generation, Origin, Presents,
    ProducerId, RunStyle, Snapshot, SourceText, StrokeMethod, StyleRun, TextRuns, Translation,
    Typography, ValueChangeKind, char_range_to_byte_range, rich_text,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    project::{CurrentProject, Project},
    redesign::EffectResolver,
};

pub(crate) use koharu_scene::rich_text::RunStyleField;

/// Fields in `clear` return to the box's style, including when also in `set`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
pub(crate) struct RunStylePatch {
    pub set: RunStyle,
    pub clear: Vec<RunStyleField>,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_word_style(
    layer: EntityId,
    start: u32,
    end: u32,
    patch: RunStylePatch,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<(), Error> {
    let (commit, page) = {
        let mut projects = project.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let commit = project.set_word_style(layer, start, end, patch).await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, project.active_page())
    };
    publish(&desktop, &canvas_channel, commit, page).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn clear_word_styles(
    layers: Vec<EntityId>,
    range: Option<[u32; 2]>,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<(), Error> {
    let (commit, page) = {
        let mut projects = project.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let commit = project.clear_word_styles(&layers, range).await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, project.active_page())
    };
    publish(&desktop, &canvas_channel, commit, page).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn replace_text_range(
    layer: EntityId,
    start: u32,
    end: u32,
    insert: String,
    // On-page typing passes its editing session so the session's chunks undo together.
    typing: Option<u32>,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<(), Error> {
    let (commit, page) = {
        let mut projects = project.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let commit = project
            .replace_text_range(layer, start, end, insert)
            .await?;
        if let Some(commit) = &commit {
            project.record_typing(layer, typing, commit);
        }
        (commit, project.active_page())
    };
    publish(&desktop, &canvas_channel, commit, page).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_word_styles(
    layer: EntityId,
    project: State<'_, CurrentProject>,
) -> Result<Vec<StyleRun>, Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    Ok(project.word_styles(layer)?)
}

async fn publish(
    desktop: &Desktop,
    canvas_channel: &CanvasChannel,
    commit: Option<Commit>,
    page: Option<EntityId>,
) -> anyhow::Result<()> {
    if let Some(commit) = commit {
        desktop.synchronize(&commit.snapshot, page, &commit).await?;
        canvas_channel.channel.publish(desktop.canvas_state());
    }
    Ok(())
}

fn shown_text(snapshot: &Snapshot, layer: EntityId) -> koharu_scene::Result<String> {
    let content = snapshot.text_layer(layer)?.content()?;
    Ok(if let Some(translation) = content.translation()? {
        translation.text.value
    } else {
        content
            .source()?
            .map(|source| source.text.value)
            .unwrap_or_default()
    })
}

fn write_runs(
    edit: &mut Edit,
    snapshot: &Snapshot,
    layer: EntityId,
    runs: Vec<StyleRun>,
    origin: Origin,
) -> koharu_scene::Result<()> {
    if runs.is_empty() {
        edit.remove::<TextRuns>(layer)?;
    } else {
        let value = TextRuns { origin, runs };
        if snapshot.component::<TextRuns>(layer)?.as_ref() != Some(&value) {
            edit.set(layer, &value)?;
        }
    }
    Ok(())
}

impl Project {
    pub(crate) fn word_styles(&self, layer: EntityId) -> anyhow::Result<Vec<StyleRun>> {
        let snapshot = self.snapshot();
        let len = shown_text(&snapshot, layer)?.chars().count() as u32;
        Ok(snapshot
            .component::<TextRuns>(layer)?
            .map_or_else(Vec::new, |value| rich_text::clamp(&value.runs, len)))
    }

    pub(crate) async fn set_word_style(
        &mut self,
        layer: EntityId,
        start: u32,
        end: u32,
        patch: RunStylePatch,
    ) -> anyhow::Result<Option<Commit>> {
        let snapshot = self.snapshot();
        let len = shown_text(&snapshot, layer)?.chars().count() as u32;
        ensure!(
            start < end && end <= len,
            "word style range is outside the shown text"
        );
        if !patch.set.is_empty() {
            patch.set.validate()?;
        }
        let current = snapshot.component::<TextRuns>(layer)?;
        let runs = rich_text::clamp(
            current.as_ref().map_or(&[], |value| value.runs.as_slice()),
            len,
        );
        let runs = rich_text::apply_patch(&runs, start, end, &patch.set, &patch.clear);
        let next = (!runs.is_empty()).then_some(TextRuns {
            origin: Origin::User,
            runs,
        });
        if current == next {
            return Ok(None);
        }
        let patch = snapshot.patch(|edit| {
            edit.promote_entity_to_user(layer)?;
            write_runs(
                edit,
                &snapshot,
                layer,
                next.map_or_else(Vec::new, |value| value.runs),
                Origin::User,
            )
        })?;
        Ok(Some(self.commit(patch).await?))
    }

    pub(crate) async fn clear_word_styles(
        &mut self,
        layers: &[EntityId],
        range: Option<[u32; 2]>,
    ) -> anyhow::Result<Option<Commit>> {
        if let Some([start, end]) = range {
            ensure!(
                start < end,
                "word style range must be non-empty and ordered"
            );
        }
        let snapshot = self.snapshot();
        let mut updates = Vec::new();
        for &layer in layers.iter().collect::<BTreeSet<_>>() {
            let len = shown_text(&snapshot, layer)?.chars().count() as u32;
            let Some(current) = snapshot.component::<TextRuns>(layer)? else {
                continue;
            };
            let runs = match range {
                Some([start, end]) => rich_text::clear_range(
                    &rich_text::clamp(&current.runs, len),
                    start.min(len),
                    end.min(len),
                ),
                None => Vec::new(),
            };
            if runs != current.runs || runs.is_empty() {
                updates.push((layer, runs));
            }
        }
        let patch = snapshot.patch(|edit| {
            for (layer, runs) in updates {
                edit.promote_entity_to_user(layer)?;
                write_runs(edit, &snapshot, layer, runs, Origin::User)?;
            }
            Ok(())
        })?;
        if patch.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.commit(patch).await?))
    }

    pub(crate) async fn replace_text_range(
        &mut self,
        layer: EntityId,
        start: u32,
        end: u32,
        insert: String,
    ) -> anyhow::Result<Option<Commit>> {
        let snapshot = self.snapshot();
        let content = snapshot.text_layer(layer)?.content()?;
        let old = shown_text(&snapshot, layer)?;
        let len = old.chars().count() as u32;
        ensure!(
            start <= end && end <= len,
            "text replacement range is outside the shown text"
        );
        ensure!(!insert.contains('\0'), "text contains NUL");
        let bytes = char_range_to_byte_range(&old, start, end);
        ensure!(
            old.len() - bytes.len() + insert.len() <= 16 * 1024 * 1024,
            "text is too large"
        );
        let mut text = old.clone();
        text.replace_range(bytes, &insert);
        if text == old {
            return Ok(None);
        }
        let patch = snapshot.patch(|edit| {
            edit.promote_entity_to_user(layer)?;
            edit.promote_entity_to_user(content.id())?;
            let unread = content
                .source()?
                .is_none_or(|source| source.text.value.is_empty());
            if let Some(translation) = content.translation()? {
                edit.set(
                    content.id(),
                    &Translation {
                        text: Authored::user(text),
                        language: translation.language,
                    },
                )?;
            } else if unread {
                // A box with nothing read into it (a new hand-drawn box) is
                // lettering: only the translation is drawn on the page.
                if content.source()?.is_none() {
                    edit.set(
                        content.id(),
                        &SourceText {
                            text: Authored::user(String::new()),
                            language: None,
                        },
                    )?;
                }
                edit.set(
                    content.id(),
                    &Translation {
                        text: Authored::user(text),
                        language: None,
                    },
                )?;
            } else {
                edit.set(
                    content.id(),
                    &SourceText {
                        text: Authored::user(text),
                        language: content.source()?.and_then(|source| source.language),
                    },
                )?;
            }
            for relation in snapshot.relations_to_as::<Presents>(content.id()) {
                let sibling = relation.value().source;
                if let Some(current) = snapshot.component::<TextRuns>(sibling)? {
                    let runs = rich_text::replace(
                        &rich_text::clamp(&current.runs, len),
                        start,
                        end,
                        insert.chars().count() as u32,
                    );
                    write_runs(edit, &snapshot, sibling, runs, current.origin)?;
                }
            }
            Ok(())
        })?;
        Ok(Some(self.commit(patch).await?))
    }
}

/// All presentations of shared content see the same edit, so their offsets
/// must move together even when only one box was edited in the panel.
pub(crate) fn on_text_edit(
    edit: &mut Edit,
    snapshot: &Snapshot,
    layer: EntityId,
    new_shown: &str,
) -> koharu_scene::Result<()> {
    let old = shown_text(snapshot, layer)?;
    if old == new_shown {
        return Ok(());
    }
    let content = snapshot.text_layer(layer)?.content()?.id();
    for relation in snapshot.relations_to_as::<Presents>(content) {
        let sibling = relation.value().source;
        if let Some(current) = snapshot.component::<TextRuns>(sibling)? {
            let runs = rich_text::remap(&current.runs, &old, new_shown);
            write_runs(edit, snapshot, sibling, runs, current.origin)?;
        }
    }
    Ok(())
}

fn translated_layers(snapshot: &Snapshot, commit: &Commit) -> koharu_scene::Result<Vec<EntityId>> {
    let mut layers = BTreeSet::new();
    for change in &commit.changes.components {
        if change.kind != Translation::KIND || change.change == ValueChangeKind::Removed {
            continue;
        }
        if let ComponentOwner::Entity(content) = change.owner {
            for relation in snapshot.relations_to_as::<Presents>(content) {
                let layer = relation.value().source;
                snapshot.text_layer(layer)?;
                layers.insert(layer);
            }
        }
    }
    Ok(layers.into_iter().collect())
}

fn default_typography(
    current: Option<&Typography>,
    translation: &Translation,
    detect_colors: bool,
) -> Typography {
    let generated = match &translation.text.origin {
        Origin::Generated(generation) => generation.clone(),
        Origin::User => Generation::new(
            ProducerId::new("dev.koharu.pipeline.translation").expect("valid producer"),
        ),
    };
    Typography {
        origin: current
            .map(|value| &value.origin)
            .filter(|origin| matches!(origin, Origin::Generated(_)))
            .cloned()
            .unwrap_or(Origin::Generated(generated)),
        preferred_font: None,
        font_weight: None,
        font_style: None,
        size: None,
        auto_fit: true,
        // Paint and direction have no separate persisted detector baseline;
        // use the same inheritance as apply_shape_rules / EffectResolver.
        color: current
            .filter(|_| detect_colors)
            .and_then(|value| value.color),
        stroke_color: current
            .filter(|_| detect_colors)
            .and_then(|value| value.stroke_color),
        stroke_width: current
            .filter(|_| detect_colors)
            .and_then(|value| value.stroke_width),
        stroke_method: Some(StrokeMethod::Contour),
        stroke_position: None,
        stroke_join: None,
        outer_stroke_color: None,
        outer_stroke_width: None,
        line_spacing: None,
        letter_spacing: None,
        fill_gradient: None,
        alignment: None,
        writing_mode: current.and_then(|value| value.writing_mode),
        extensions: Default::default(),
    }
}

async fn reset_translation_styles(
    project: &mut Project,
    commit: &Commit,
    styles: &TextStyles,
    detect_colors: bool,
    apply_hints: impl FnOnce(&mut Edit, &Snapshot, &[EntityId]) -> koharu_scene::Result<()>,
) -> anyhow::Result<Option<Commit>> {
    let snapshot = project.snapshot();
    let layers = translated_layers(&snapshot, commit)?;
    if layers.is_empty() {
        return Ok(None);
    }
    styles.validate()?;
    let resolver = EffectResolver::new(project, styles, detect_colors)?;
    let mut updates = Vec::new();
    for &layer in &layers {
        let translation = snapshot
            .text_layer(layer)?
            .content()?
            .translation()?
            .context("translation was removed")?;
        let current = snapshot.component::<Typography>(layer)?;
        let baseline = default_typography(current.as_ref(), &translation, detect_colors);
        let typography = resolver.resolve(&snapshot, layer, &baseline)?;
        let mut layout = snapshot.text_layer(layer)?.layout()?;
        let warped = layout.warp.take().is_some();
        if warped {
            layout.origin = baseline.origin;
        }
        updates.push((layer, typography, layout, warped));
    }
    let patch = snapshot.patch(|edit| {
        for (layer, typography, layout, warped) in updates {
            edit.remove::<TextRuns>(layer)?;
            if snapshot.component::<Typography>(layer)?.as_ref() != Some(&typography) {
                edit.reset_typography(layer, &typography)?;
            }
            if warped {
                edit.set(layer, &layout)?;
            }
        }
        // Hints are fresh translation output. Emphasis may replace the runs
        // just removed, within this same undo step.
        apply_hints(edit, &snapshot, &layers)
    })?;
    if patch.is_empty() {
        return Ok(None);
    }
    Ok(Some(project.commit(patch).await?))
}

/// Called by the processing committer right after a Translation stage commit
/// (normal runs and Translate again). For every box whose translation that
/// commit rewrote, removes its word styles and resets its style to the lettering
/// default (decision 4), then applies fresh emphasis hints in the same patch.
pub(crate) async fn after_translation(
    project: &mut Project,
    commit: &Commit,
) -> anyhow::Result<Option<Commit>> {
    if translated_layers(&project.snapshot(), commit)?.is_empty() {
        return Ok(None);
    }
    let styles = super::styles::load_text_styles()?.read()?.clone();
    let detect_colors = koharu_pipeline::PipelineConfig::load()?
        .read()?
        .detect_text_color;
    // Lettering labels live in the sidecar, not the scene; load them before the patch.
    let context = project.load_context()?;
    reset_translation_styles(
        project,
        commit,
        &styles,
        detect_colors,
        |edit, snapshot, layers| super::emphasis::apply_hints(edit, snapshot, layers, &context),
    )
    .await
}
