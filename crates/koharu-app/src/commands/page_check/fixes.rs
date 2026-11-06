use anyhow::{Context as _, Result, ensure};
use koharu_desktop::Desktop;
use koharu_renderer::{TextStyle, TextStyles};
use koharu_scene::{Commit, EntityId, Origin, Snapshot, TextRuns, Typography};
use tauri::{AppHandle, Manager as _};
use tauri_runtime_cef::CefRuntime;

use super::{PageCheckFix, PageCheckKind, PageCheckPage, matching, stored_report};
use crate::commands::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    processing::Processing,
    project::{CurrentProject, Project},
};

fn outlined(
    snapshot: &Snapshot,
    layer: EntityId,
    styles: &TextStyles,
) -> Result<Option<Typography>> {
    let text = snapshot.text_layer(layer)?;
    // A warped glyph can leave its frame; that correspondence needs rendered glyph bounds.
    if text.layout()?.warp.is_some() || text.frame()?.is_none() {
        return Ok(None);
    }
    if !text
        .content()?
        .translation()?
        .is_some_and(|translation| !translation.text.value.trim().is_empty())
    {
        return Ok(None);
    }
    let Some(original) = text.typography()? else {
        return Ok(None);
    };
    if original.origin == Origin::User
        || snapshot
            .component::<TextRuns>(layer)?
            .is_some_and(|runs| !runs.runs.is_empty())
    {
        return Ok(None);
    }
    // A page check changes only the configured free-text outlines, not font, color or layout.
    let free = &styles.shape_styles.free;
    let style = TextStyle {
        outline: free.outline.clone(),
        outer_outline: free.outer_outline.clone(),
        ..Default::default()
    };
    style.validate()?;
    let mut updated = original.clone();
    style.apply_to(&mut updated);
    // The renderer draws a second outline only when the first has positive width.
    let visible = updated.stroke_width.is_some_and(|width| width > 0.0)
        && (updated.stroke_color.is_none_or(|color| color[3] > 0)
            || (updated.outer_stroke_width.is_some_and(|width| width > 0.0)
                && updated.outer_stroke_color.is_none_or(|color| color[3] > 0)));
    if !visible {
        return Ok(None);
    }
    Ok((updated != original).then_some(updated))
}

pub(super) fn outline_available(
    snapshot: &Snapshot,
    layer: EntityId,
    styles: &TextStyles,
) -> Result<bool> {
    Ok(outlined(snapshot, layer, styles)?.is_some())
}

async fn apply_outline(
    project: &mut Project,
    layer: EntityId,
    styles: &TextStyles,
    reason: &str,
) -> Result<Option<Commit>> {
    let snapshot = project.snapshot();
    let Some(mut typography) = outlined(&snapshot, layer, styles)? else {
        return Ok(None);
    };
    typography
        .extensions
        .insert("page_check_reason".into(), reason.into());
    let patch = snapshot.patch(|edit| edit.set_typography_preserving_origin(layer, &typography))?;
    Ok(Some(project.commit(patch).await?))
}

fn fix_target(
    snapshot: &Snapshot,
    checked: &PageCheckPage,
    finding_id: &str,
) -> Result<(EntityId, String)> {
    ensure!(
        checked.revision == snapshot.revision(),
        "page check is stale; check the page again"
    );
    let finding = checked
        .findings
        .iter()
        .find(|finding| finding.id == finding_id)
        .context("page check finding no longer exists")?;
    ensure!(
        matches!(finding.kind, PageCheckKind::HardToRead)
            && matches!(finding.fix, Some(PageCheckFix::Outline)),
        "this finding has no reliable fix"
    );
    let layer = finding
        .matched_layer
        .context("page check has no concrete text match")?;
    ensure!(
        matching::matched_layer(snapshot, checked.page, finding.area)? == Some(layer),
        "page check no longer matches a single text area"
    );
    Ok((layer, finding.detail.clone()))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn apply_page_check_fix(
    handle: AppHandle<CefRuntime>,
    page: EntityId,
    finding_id: String,
) -> Result<bool, Error> {
    if !handle.state::<Processing>().stops.lock().is_empty() {
        return Err(anyhow::anyhow!("another process is already running").into());
    }
    if handle.state::<crate::commands::runs::BatchState>().active() {
        return Err(anyhow::anyhow!("a chapter batch is running").into());
    }
    let styles = crate::commands::styles::load_text_styles()?.read()?.clone();
    let (commit, active) = {
        let projects = handle.state::<CurrentProject>();
        let mut projects = projects.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let snapshot = project.snapshot();
        let report = stored_report(&project.load_context()?)?;
        let checked = report
            .pages
            .iter()
            .find(|result| result.page == page)
            .context("page has not been checked")?;
        let (layer, reason) = fix_target(&snapshot, checked, &finding_id)?;
        let commit = apply_outline(project, layer, &styles, &reason).await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, project.active_page())
    };
    let Some(commit) = commit else {
        return Ok(false);
    };
    let desktop = handle.state::<Desktop>();
    desktop.synchronize(&commit.snapshot, active, &commit).await?;
    handle
        .state::<CanvasChannel>()
        .channel
        .publish(desktop.canvas_state());
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::page_check::{PageCheckFinding, PageCheckStatus};
    use koharu_pipeline::Bounds;
    use koharu_renderer::OutlinePart;
    use koharu_scene::{
        At, Authored, Generation, Geometry, PageDraft, ProducerId, RunStyle, StyleRun, TextLayout,
        TextLayoutKind, Translation,
    };

    async fn fixture() -> (Project, EntityId) {
        let mut session = koharu_scene::Session::memory().await.unwrap();
        let generation = Generation::new(ProducerId::new("dev.koharu.pipeline.detection").unwrap());
        let mut edit = session.snapshot().edit_as(generation.clone());
        let page = edit
            .add_page(PageDraft::new("page", 500., 500.), At::End)
            .unwrap();
        let content = edit.add_text_content(page, At::End).unwrap();
        edit.set(
            content,
            &Translation {
                text: Authored::generated("text".into(), generation.clone()),
                language: None,
            },
        )
        .unwrap();
        let layer = edit
            .add_text_layer(
                page,
                At::End,
                content,
                &TextLayout {
                    origin: Origin::Generated(generation.clone()),
                    kind: TextLayoutKind::Paragraph,
                    angle_degrees: None,
                    warp: None,
                },
            )
            .unwrap();
        edit.set(layer, &Geometry::rectangle(10., 10., 100., 50.))
            .unwrap();
        edit.set(
            layer,
            &Typography {
                origin: Origin::Generated(generation),
                preferred_font: Some("original".into()),
                font_weight: None,
                font_style: None,
                size: None,
                auto_fit: true,
                color: Some([12, 34, 56, 255]),
                stroke_color: None,
                stroke_width: None,
                alignment: None,
                writing_mode: None,
                extensions: Default::default(),
                stroke_method: None,
                stroke_position: None,
                stroke_join: None,
                outer_stroke_color: None,
                outer_stroke_width: None,
                line_spacing: None,
                letter_spacing: None,
                fill_gradient: None,
            },
        )
        .unwrap();
        session.commit(edit.finish().unwrap()).await.unwrap();
        (Project::new(session, "page check".into()), layer)
    }

    fn styles() -> TextStyles {
        let mut styles = TextStyles::default();
        styles.shape_styles.free.outline = Some(OutlinePart {
            stroke_width: Some(3.),
            stroke_color: Some([255, 255, 255, 255]),
            stroke_method: None,
            stroke_position: None,
            stroke_join: None,
        });
        styles.shape_styles.free.fill = Some([0, 0, 0, 255]);
        styles
    }

    #[tokio::test]
    async fn outline_is_one_undo_step_and_repeating_it_does_nothing() {
        let (mut project, layer) = fixture().await;
        let original = project
            .snapshot()
            .component::<Typography>(layer)
            .unwrap()
            .unwrap();
        let commit = apply_outline(&mut project, layer, &styles(), "busy artwork")
            .await
            .unwrap()
            .unwrap();
        project.record_commit(&commit);
        let after = project
            .snapshot()
            .component::<Typography>(layer)
            .unwrap()
            .unwrap();
        assert_eq!(after.stroke_width, Some(3.));
        assert_eq!(after.preferred_font, original.preferred_font);
        assert_eq!(after.color, original.color);
        assert_eq!(after.origin, original.origin);
        assert_eq!(project.undo.len(), 1);
        assert!(
            apply_outline(&mut project, layer, &styles(), "another description")
                .await
                .unwrap()
                .is_none()
        );
        project.undo().await.unwrap();
        assert_eq!(
            project
                .snapshot()
                .component::<Typography>(layer)
                .unwrap()
                .unwrap(),
            original
        );
    }

    #[tokio::test]
    async fn hand_styling_and_word_styles_are_never_replaced() {
        let (mut project, layer) = fixture().await;
        let snapshot = project.snapshot();
        let mut typography = snapshot.component::<Typography>(layer).unwrap().unwrap();
        typography.stroke_width = Some(7.);
        let patch = snapshot.patch(|edit| edit.set(layer, &typography)).unwrap();
        project.commit(patch).await.unwrap();
        assert!(
            apply_outline(&mut project, layer, &styles(), "busy artwork")
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            project
                .snapshot()
                .component::<Typography>(layer)
                .unwrap()
                .unwrap()
                .stroke_width,
            Some(7.)
        );

        let (mut project, layer) = fixture().await;
        let patch = project
            .snapshot()
            .patch(|edit| {
                edit.set(
                    layer,
                    &TextRuns {
                        origin: Origin::User,
                        runs: vec![StyleRun {
                            start: 0,
                            end: 2,
                            style: RunStyle {
                                color: Some([255, 0, 0, 255]),
                                ..Default::default()
                            },
                        }],
                    },
                )
            })
            .unwrap();
        project.commit(patch).await.unwrap();
        assert!(
            apply_outline(&mut project, layer, &styles(), "busy artwork")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn absent_or_invisible_free_text_outline_has_no_fix() {
        let (project, layer) = fixture().await;
        let snapshot = project.snapshot();
        assert!(!outline_available(&snapshot, layer, &TextStyles::default()).unwrap());
        let mut styles = styles();
        styles.shape_styles.free.outline.as_mut().unwrap().stroke_color = Some([0, 0, 0, 0]);
        assert!(!outline_available(&snapshot, layer, &styles).unwrap());
    }

    #[tokio::test]
    async fn a_saved_fix_requires_the_same_revision_and_concrete_match() {
        let (mut project, layer) = fixture().await;
        let snapshot = project.snapshot();
        let page = snapshot.pages().next().unwrap().id();
        let mut checked = PageCheckPage {
            page,
            page_label: "page".into(),
            revision: snapshot.revision(),
            status: PageCheckStatus::Checked,
            reason: None,
            findings: vec![PageCheckFinding {
                id: "finding".into(),
                kind: PageCheckKind::HardToRead,
                detail: "busy artwork".into(),
                area: Bounds {
                    x: 10.,
                    y: 10.,
                    width: 100.,
                    height: 50.,
                },
                matched_layer: Some(layer),
                fix: Some(PageCheckFix::Outline),
            }],
        };
        assert_eq!(fix_target(&snapshot, &checked, "finding").unwrap().0, layer);
        assert!(fix_target(&snapshot, &checked, "replaced finding").is_err());
        checked.findings[0].matched_layer = Some(EntityId::new());
        assert!(fix_target(&snapshot, &checked, "finding").is_err());
        checked.findings[0].matched_layer = Some(layer);
        checked.findings[0].area.x = 300.;
        assert!(fix_target(&snapshot, &checked, "finding").is_err());
        checked.findings[0].area.x = 10.;

        let patch = snapshot
            .patch(|edit| edit.set(layer, &Geometry::rectangle(15., 10., 100., 50.)))
            .unwrap();
        project.commit(patch).await.unwrap();
        // Even a small edit whose area still matches invalidates the model's observation.
        assert!(fix_target(&project.snapshot(), &checked, "finding").is_err());
    }
}
