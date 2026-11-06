use anyhow::Result;
use booharu_context::ProjectContext;
use koharu_pipeline::{Bounds, SFX_ACTION_KEY};
use koharu_scene::{EntityId, Snapshot};

use super::{LetteringSettings, Project, effective, subtitle_source};

/// Labels and settings live outside scene revisions. Check them while holding
/// the project lock, before accepting cleanup derived from the stamped policy.
pub(crate) fn cleanup_policy_current(
    project: &Project,
    page: EntityId,
    bounds: Option<Bounds>,
) -> Result<bool> {
    let settings = LetteringSettings::load()?.read()?.clone();
    cleanup_policy_matches(&project.load_context()?, &project.snapshot(), page, &settings, bounds)
}

fn cleanup_policy_matches(
    context: &ProjectContext,
    snapshot: &Snapshot,
    page: EntityId,
    settings: &LetteringSettings,
    bounds: Option<Bounds>,
) -> Result<bool> {
    let Some(group) = snapshot.page(page)?.text_group()? else {
        return Ok(true);
    };
    for layer in group.text_layers()? {
        if subtitle_source(snapshot, layer.id())?.is_some() {
            continue;
        }
        if let Some(bounds) = bounds {
            let geometry = if let Some(region) = layer.content()?.source_region()? {
                Some(region.geometry()?)
            } else {
                layer.frame()?
            };
            let Some(geometry) = geometry else {
                continue;
            };
            let intersects = geometry.points.iter().any(|point| point.x < bounds.x + bounds.width)
                && geometry.points.iter().any(|point| point.x > bounds.x)
                && geometry.points.iter().any(|point| point.y < bounds.y + bounds.height)
                && geometry.points.iter().any(|point| point.y > bounds.y);
            if !intersects {
                continue;
            }
        }
        let expected = effective(context, snapshot, layer.id(), settings)?
            .map(|decision| decision.action.as_str());
        let current = layer
            .typography()?
            .and_then(|style| style.extensions.get(SFX_ACTION_KEY).cloned());
        if current.as_deref() != expected {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{CHOICE, LetteringCategory, SfxOutsideMode};
    use koharu_scene::Geometry;

    #[tokio::test]
    async fn cleanup_rejects_changed_labels_importance_and_settings_without_a_scene_revision() {
        let (_directory, mut project, layers) =
            crate::commands::lettering::tests::project_fixture().await;
        let layer = layers[2];
        let page = project.snapshot().pages().next().unwrap().id();
        let patch = project
            .snapshot()
            .patch(|edit| edit.set(layer, &Geometry::rectangle(300.0, 100.0, 60.0, 100.0)))
            .unwrap();
        project.commit(patch).await.unwrap();
        let mut context = ProjectContext::default();
        let mut settings = LetteringSettings::default();
        assert!(cleanup_policy_matches(&context, &project.snapshot(), page, &settings, None).unwrap());
        context.set_user_category(layer, Some(LetteringCategory::Sfx));
        assert!(!cleanup_policy_matches(&context, &project.snapshot(), page, &settings, None).unwrap());
        for (bounds, expected) in [
            (Bounds { x: 0.0, y: 0.0, width: 100.0, height: 100.0 }, true),
            (Bounds { x: 300.0, y: 100.0, width: 60.0, height: 100.0 }, false),
        ] {
            assert_eq!(
                cleanup_policy_matches(&context, &project.snapshot(), page, &settings, Some(bounds)).unwrap(),
                expected
            );
        }

        let snapshot = project.snapshot();
        let patch = snapshot
            .patch(|edit| {
                let mut style = snapshot.text_layer(layer)?.typography()?.unwrap();
                style.extensions.insert(SFX_ACTION_KEY.into(), "skip".into());
                edit.set_typography_preserving_origin(layer, &style)
            })
            .unwrap();
        project.commit(patch).await.unwrap();
        let snapshot = project.snapshot();
        assert!(cleanup_policy_matches(&context, &snapshot, page, &settings, None).unwrap());
        settings.sfx_outside = SfxOutsideMode::Subtitle;
        assert!(!cleanup_policy_matches(&context, &snapshot, page, &settings, None).unwrap());
        settings.sfx_important_only = true;
        assert!(cleanup_policy_matches(&context, &snapshot, page, &settings, None).unwrap());
        context.lines.get_mut(&layer).unwrap().sfx_important = Some(true);
        assert!(!cleanup_policy_matches(&context, &snapshot, page, &settings, None).unwrap());
        context.set_user_category(layer, Some(LetteringCategory::NormalDialogue));
        assert!(!cleanup_policy_matches(&context, &snapshot, page, &settings, None).unwrap());

        context.set_user_category(layer, Some(LetteringCategory::Sfx));
        let patch = snapshot
            .patch(|edit| {
                let mut style = snapshot.text_layer(layer)?.typography()?.unwrap();
                style.extensions.insert(CHOICE.into(), "translate".into());
                style.extensions.insert(SFX_ACTION_KEY.into(), "translate".into());
                edit.set_typography_preserving_origin(layer, &style)
            })
            .unwrap();
        let snapshot = project.commit(patch).await.unwrap().snapshot;
        assert!(cleanup_policy_matches(&context, &snapshot, page, &settings, None).unwrap());
        settings.sfx_outside = SfxOutsideMode::Skip;
        assert!(cleanup_policy_matches(&context, &snapshot, page, &settings, None).unwrap());
    }
}
