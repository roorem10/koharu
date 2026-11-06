//! SFX choices and subtitle ownership are scene edits; reading labels stay in context.

#[path = "sfx_policy.rs"]
mod policy;
pub(crate) use policy::cleanup_policy_current;

#[cfg(test)]
#[path = "sfx_tests.rs"]
mod tests;

use anyhow::{Context as _, Result, ensure};
use booharu_context::{LetteringCategory, ProjectContext};
use koharu_desktop::Desktop;
use koharu_pipeline::{
    SFX_ACTION_KEY, SFX_SUBTITLE_SOURCE_KEY, Scope, Stage, bubble_shape::text_bubble_shape,
};
use koharu_renderer::TextStyles;
use koharu_scene::{
    Commit, Edit, EntityId, Generation, Origin, ProducerId, Snapshot, Typography, Visibility,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager as _, State};
use tauri_runtime_cef::CefRuntime;

use super::{
    LetteringSettings,
    settings::{SfxInsideMode, SfxOutsideMode},
};
use crate::commands::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    ocr_box::settings::OcrBoxSettings,
    project::{CurrentProject, Project},
};

const CHOICE: &str = "dev.booharu.sfx.choice";
const REASON: &str = "dev.booharu.sfx.reason";
const PRODUCER: &str = "dev.booharu.sfx";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, Type, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SfxAction {
    Skip,
    Subtitle,
    Translate,
}

impl SfxAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Skip => "skip",
            Self::Subtitle => "subtitle",
            Self::Translate => "translate",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SfxReason {
    User,
    InsideBubble,
    OutsideBubble,
    Unimportant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
pub(crate) struct EffectiveSfx {
    pub action: SfxAction,
    pub reason: SfxReason,
    pub automatic: bool,
}

fn decide(
    choice: Option<SfxAction>,
    inside: bool,
    important: Option<bool>,
    settings: &LetteringSettings,
) -> EffectiveSfx {
    let (action, reason) = if let Some(choice) = choice {
        (choice, SfxReason::User)
    } else if inside {
        (
            match settings.sfx_inside {
                SfxInsideMode::Translate => SfxAction::Translate,
                SfxInsideMode::Skip => SfxAction::Skip,
            },
            SfxReason::InsideBubble,
        )
    } else if settings.sfx_important_only && important != Some(true) {
        (SfxAction::Skip, SfxReason::Unimportant)
    } else {
        (
            match settings.sfx_outside {
                SfxOutsideMode::Skip => SfxAction::Skip,
                SfxOutsideMode::Subtitle => SfxAction::Subtitle,
                SfxOutsideMode::Replace => SfxAction::Translate,
            },
            SfxReason::OutsideBubble,
        )
    };
    EffectiveSfx {
        action,
        reason,
        automatic: choice.is_none(),
    }
}

pub(crate) fn subtitle_source(snapshot: &Snapshot, layer: EntityId) -> Result<Option<EntityId>> {
    snapshot
        .component::<Typography>(layer)?
        .and_then(|style| style.extensions.get(SFX_SUBTITLE_SOURCE_KEY).cloned())
        .map(|value| serde_json::from_value(serde_json::Value::String(value)).map_err(Into::into))
        .transpose()
}

pub(crate) fn find_subtitle(snapshot: &Snapshot, source: EntityId) -> Result<Option<EntityId>> {
    for layer in snapshot.text_layers()? {
        if subtitle_source(snapshot, layer.id())? == Some(source) {
            return Ok(Some(layer.id()));
        }
    }
    Ok(None)
}

pub(crate) fn effective(
    context: &ProjectContext,
    snapshot: &Snapshot,
    layer: EntityId,
    settings: &LetteringSettings,
) -> Result<Option<EffectiveSfx>> {
    snapshot.text_layer(layer)?;
    // A hand-set non-SFX category takes the box out of this policy altogether.
    if context
        .category(layer)
        .is_none_or(|label| label.value != LetteringCategory::Sfx)
        || subtitle_source(snapshot, layer)?.is_some()
    {
        return Ok(None);
    }
    let choice = snapshot
        .component::<Typography>(layer)?
        .and_then(|style| style.extensions.get(CHOICE).cloned())
        .map(|value| serde_json::from_value::<SfxAction>(serde_json::Value::String(value)))
        .transpose()?;
    Ok(Some(decide(
        choice,
        text_bubble_shape(snapshot, layer)?.bubble.is_some(),
        context.lines.get(&layer).and_then(|line| line.sfx_important),
        settings,
    )))
}

fn visibility(edit: &mut Edit, snapshot: &Snapshot, layer: EntityId, visible: bool) -> Result<()> {
    let current = snapshot.component::<Visibility>(layer)?;
    // Keep an explicit visibility edit. Only restore visibility that this owner hid.
    if current
        .as_ref()
        .is_some_and(|value| value.origin == Origin::User)
    {
        return Ok(());
    }
    if current.as_ref().is_some_and(|value| value.visible == visible) {
        return Ok(());
    }
    edit.set_automatic_visibility(
        layer,
        &Visibility {
            origin: Origin::Generated(Generation::new(ProducerId::new(PRODUCER)?)),
            visible,
            opacity: current.map_or(1.0, |value| value.opacity),
        },
    )?;
    Ok(())
}

/// None leaves the hand choice alone; Some(None) returns it to settings.
pub(crate) async fn apply(
    project: &mut Project,
    layer: EntityId,
    choice: Option<Option<SfxAction>>,
    subtitles: bool,
    subtitle_settings: &OcrBoxSettings,
    styles: &TextStyles,
    settings: &LetteringSettings,
) -> Result<(Option<Commit>, Option<EntityId>)> {
    let snapshot = project.snapshot();
    let context = project.load_context()?;
    let mut decision = effective(&context, &snapshot, layer, settings)?;
    let Some(mut style) = snapshot.component::<Typography>(layer)? else {
        ensure!(
            decision.is_none() && choice.is_none(),
            "text box has no typography"
        );
        return Ok((None, None));
    };
    let original = style.clone();
    if let Some(choice) = choice {
        ensure!(
            context
                .category(layer)
                .is_some_and(|label| label.value == LetteringCategory::Sfx),
            "select an SFX box"
        );
        match choice {
            Some(action) => {
                style.extensions.insert(CHOICE.into(), action.as_str().into());
            }
            None => {
                style.extensions.remove(CHOICE);
            }
        }
    }
    if let Some(choice) = choice {
        decision = Some(decide(
            choice,
            text_bubble_shape(&snapshot, layer)?.bubble.is_some(),
            context.lines.get(&layer).and_then(|line| line.sfx_important),
            settings,
        ));
    }
    let mut edit = snapshot.edit();
    let mut subtitle = find_subtitle(&snapshot, layer)?;
    if let Some(decision) = decision {
        style
            .extensions
            .insert(SFX_ACTION_KEY.into(), decision.action.as_str().into());
        style.extensions.insert(
            REASON.into(),
            serde_json::to_value(decision.reason)?
                .as_str()
                .context("invalid SFX reason")?
                .into(),
        );
        if decision.action == SfxAction::Subtitle && subtitles && subtitle.is_none() {
            let (added, _) = crate::commands::ocr_box::add_subtitle(
                &snapshot,
                &mut edit,
                layer,
                subtitle_settings,
                styles,
            )?;
            edit.set_automatic_visibility(
                added,
                &Visibility {
                    origin: Origin::Generated(Generation::new(ProducerId::new(PRODUCER)?)),
                    visible: true,
                    opacity: 1.0,
                },
            )?;
            subtitle = Some(added);
        }
        visibility(
            &mut edit,
            &snapshot,
            layer,
            decision.action == SfxAction::Translate,
        )?;
        if let Some(subtitle) = subtitle.filter(|id| snapshot.text_layer(*id).is_ok()) {
            visibility(
                &mut edit,
                &snapshot,
                subtitle,
                decision.action == SfxAction::Subtitle,
            )?;
        }
    } else {
        let was_sfx = style.extensions.remove(SFX_ACTION_KEY).is_some();
        style.extensions.remove(REASON);
        if was_sfx {
            visibility(&mut edit, &snapshot, layer, true)?;
        }
        if let Some(subtitle) = subtitle {
            visibility(&mut edit, &snapshot, subtitle, false)?;
        }
    }
    if style != original {
        edit.set_typography_preserving_origin(layer, &style)?;
    }
    let patch = edit.finish()?;
    let commit = if patch.is_empty() {
        None
    } else {
        Some(project.commit(patch).await?)
    };
    Ok((commit, subtitle))
}

pub(crate) async fn set_choice(
    project: &mut Project,
    layer: EntityId,
    action: Option<SfxAction>,
) -> Result<(Option<Commit>, EffectiveSfx)> {
    let settings = LetteringSettings::load()?.read()?.clone();
    let subtitle_settings = OcrBoxSettings::load()?;
    let styles = crate::commands::styles::load_text_styles()?.read()?.clone();
    let (commit, _) = apply(
        project,
        layer,
        Some(action),
        true,
        &subtitle_settings,
        &styles,
        &settings,
    )
    .await?;
    let decision = effective(
        &project.load_context()?,
        &project.snapshot(),
        layer,
        &settings,
    )?
    .context("select an SFX box")?;
    Ok((commit, decision))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_sfx_handling(
    layer: EntityId,
    project: State<'_, CurrentProject>,
) -> Result<Option<EffectiveSfx>, Error> {
    let settings = LetteringSettings::load()?.read()?.clone();
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    Ok(effective(
        &project.load_context()?,
        &project.snapshot(),
        layer,
        &settings,
    )?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_sfx_handling(
    layer: EntityId,
    action: Option<SfxAction>,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<EffectiveSfx, Error> {
    let (commit, decision, page) = {
        let mut projects = project.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let (commit, decision) = set_choice(project, layer, action).await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, decision, project.active_page())
    };
    if let Some(commit) = commit {
        desktop.synchronize(&commit.snapshot, page, &commit).await?;
        canvas_channel.channel.publish(desktop.canvas_state());
    }
    Ok(decision)
}

/// Resolve after chapter reading and before the pass snapshots its masks and targets.
pub(crate) async fn prepare(
    handle: &AppHandle<CefRuntime>,
    expected: koharu_scene::ProjectId,
    request: &mut koharu_pipeline::Request,
) -> Result<()> {
    let stages = request.operation.stages()?;
    let subtitles = stages.contains(&Stage::Translation);
    if request.inpainting_mask.is_some() && !subtitles {
        return Ok(());
    }
    if !subtitles && !stages.contains(&Stage::Inpainting) {
        return Ok(());
    }
    let settings = LetteringSettings::load()?.read()?.clone();
    let subtitle_settings = OcrBoxSettings::load()?;
    let styles = crate::commands::styles::load_text_styles()?.read()?.clone();
    let (commits, active) = {
        let projects = handle.state::<CurrentProject>();
        let mut projects = projects.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let snapshot = project.snapshot();
        ensure!(
            snapshot.project_id() == expected,
            "the project changed during processing"
        );
        let mut targets = Vec::new();
        for page in snapshot.pages() {
            let included = match &request.scope {
                Scope::Project | Scope::Entities(_) => true,
                Scope::Pages(pages) => pages.contains(&page.id()),
                Scope::Region { page: id, .. } => *id == page.id(),
            };
            if !included {
                continue;
            }
            if let Some(group) = page.text_group()? {
                for layer in group.text_layers()? {
                    if let Scope::Entities(ids) = &request.scope
                        && !ids.contains(&layer.id())
                    {
                        continue;
                    }
                    if let Scope::Region { bounds, .. } = &request.scope {
                        let geometry = if let Some(region) = layer.content()?.source_region()? {
                            Some(region.geometry()?)
                        } else {
                            layer.frame()?
                        };
                        let Some(geometry) = geometry else {
                            continue;
                        };
                        let intersects = geometry
                            .points
                            .iter()
                            .any(|point| point.x < bounds.x + bounds.width)
                            && geometry.points.iter().any(|point| point.x > bounds.x)
                            && geometry
                                .points
                                .iter()
                                .any(|point| point.y < bounds.y + bounds.height)
                            && geometry.points.iter().any(|point| point.y > bounds.y);
                        if !intersects {
                            continue;
                        }
                    }
                    if subtitle_source(&snapshot, layer.id())?.is_none() {
                        targets.push(layer.id());
                    }
                }
            }
        }
        let mut commits = Vec::new();
        for layer in targets {
            if request.stop.stopped() {
                break;
            }
            let (commit, subtitle) = apply(
                project,
                layer,
                None,
                subtitles,
                &subtitle_settings,
                &styles,
                &settings,
            )
            .await?;
            if let Some(commit) = commit {
                project.record_commit(&commit);
                commits.push(commit);
            }
            if let Some(subtitle) = subtitle
                && let Scope::Entities(ids) = &mut request.scope
                && !ids.contains(&subtitle)
            {
                ids.push(subtitle);
            }
        }
        (commits, project.active_page())
    };
    let desktop = handle.state::<Desktop>();
    for commit in commits {
        desktop.synchronize(&commit.snapshot, active, &commit).await?;
    }
    handle
        .state::<CanvasChannel>()
        .channel
        .publish(desktop.canvas_state());
    Ok(())
}
