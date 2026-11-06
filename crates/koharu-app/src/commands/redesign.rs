//! WO-23: box effects and the panels and popups of the redesigned editor.

pub(crate) mod agent;
mod effects;
#[cfg(test)]
mod tests;

use anyhow::Context as _;
use koharu_desktop::Desktop;
use koharu_pipeline::bubble_shape::text_bubble_shape;
use koharu_renderer::TextStyles;
use koharu_scene::{Commit, EntityId, Origin, Snapshot, TextLayout, TextRuns, Typography};
use tauri::State;

use super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    project::{CurrentProject, Project},
};
use effects::{automatic_effects, copy_effects, effects_differ, strip_word_effects, word_effects};

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_box_effects(
    layers: Vec<EntityId>,
    project: State<'_, CurrentProject>,
) -> Result<Vec<bool>, Error> {
    let styles = super::styles::load_text_styles()?.read()?.clone();
    let detect_colors = koharu_pipeline::PipelineConfig::load()?
        .read()?
        .detect_text_color;
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    let snapshot = project.snapshot();
    let resolver = EffectResolver::new(project, &styles, detect_colors)?;
    layers
        .into_iter()
        .map(|layer| resolver.has_effects(&snapshot, layer))
        .collect::<anyhow::Result<_>>()
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn reset_box_effects(
    layers: Vec<EntityId>,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<(), Error> {
    let styles = super::styles::load_text_styles()?.read()?.clone();
    let detect_colors = koharu_pipeline::PipelineConfig::load()?
        .read()?
        .detect_text_color;
    let (commit, page) = {
        let mut projects = project.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let commit = project
            .reset_effects(&layers, &styles, detect_colors)
            .await?;
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

pub(crate) struct EffectResolver<'a> {
    styles: &'a TextStyles,
    detect_colors: bool,
    category: Option<(
        booharu_context::ProjectContext,
        super::lettering::LetteringSettings,
    )>,
}

impl<'a> EffectResolver<'a> {
    fn has_effects(&self, snapshot: &Snapshot, layer: EntityId) -> anyhow::Result<bool> {
        snapshot.text_layer(layer)?;
        let typography = snapshot
            .component::<Typography>(layer)?
            .context("text layer has no typography")?;
        let expected = self.resolve(snapshot, layer, &typography)?;
        let warp = snapshot
            .component::<TextLayout>(layer)?
            .and_then(|layout| layout.warp);
        let runs = snapshot.component::<TextRuns>(layer)?;
        Ok(effects_differ(&typography, &expected, warp.as_ref())
            || runs.as_ref().is_some_and(word_effects))
    }

    pub(crate) fn new(
        project: &Project,
        styles: &'a TextStyles,
        detect_colors: bool,
    ) -> anyhow::Result<Self> {
        let category = if styles.shape_styles.enabled {
            Some((
                project.load_context()?,
                super::lettering::LetteringSettings::load()?.read()?.clone(),
            ))
        } else {
            None
        };
        Ok(Self {
            styles,
            detect_colors,
            category,
        })
    }

    pub(crate) fn resolve(
        &self,
        snapshot: &Snapshot,
        layer: EntityId,
        current: &Typography,
    ) -> anyhow::Result<Typography> {
        let category = if let Some((context, settings)) = &self.category {
            let info = text_bubble_shape(snapshot, layer)?;
            Some(
                super::lettering::resolve_category(context, snapshot, layer, &info, settings)?
                    .category,
            )
        } else {
            None
        };
        Ok(automatic_effects(
            current,
            self.styles,
            category.map(|value| value.as_str()),
            self.detect_colors,
        ))
    }
}

impl Project {
    async fn reset_effects(
        &mut self,
        layers: &[EntityId],
        styles: &TextStyles,
        detect_colors: bool,
    ) -> anyhow::Result<Option<Commit>> {
        let snapshot = self.snapshot();
        let resolver = EffectResolver::new(self, styles, detect_colors)?;
        let mut updates = Vec::new();
        for &layer in layers.iter().collect::<std::collections::BTreeSet<_>>() {
            snapshot.text_layer(layer)?;
            let original = snapshot
                .component::<Typography>(layer)?
                .context("text layer has no typography")?;
            let expected = resolver.resolve(&snapshot, layer, &original)?;
            let mut typography = original.clone();
            copy_effects(&mut typography, &expected);
            let mut layout = snapshot
                .component::<TextLayout>(layer)?
                .context("text layer has no layout")?;
            let warped = layout.warp.take().is_some();
            let changed = typography != original;
            let runs = snapshot
                .component::<TextRuns>(layer)?
                .filter(word_effects)
                .map(strip_word_effects);
            if !changed && !warped && runs.is_none() {
                continue;
            }
            if changed {
                typography.origin = Origin::User;
            }
            if warped {
                layout.origin = Origin::User;
            }
            updates.push((layer, typography, layout, changed, warped, runs));
        }
        if updates.is_empty() {
            return Ok(None);
        }
        let patch = snapshot.patch(|edit| {
            for (layer, typography, layout, changed, warped, runs) in &updates {
                if *changed {
                    edit.set(*layer, typography)?;
                }
                if *warped {
                    edit.set(*layer, layout)?;
                }
                if let Some(runs) = runs {
                    if runs.runs.is_empty() {
                        edit.remove::<TextRuns>(*layer)?;
                    } else {
                        edit.set(*layer, runs)?;
                    }
                }
            }
            Ok(())
        })?;
        Ok(Some(self.commit(patch).await?))
    }
}
