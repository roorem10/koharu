use super::*;
use koharu_scene::Geometry;

#[test]
fn automatic_sfx_policy_separates_bubbles_and_story_importance() {
    let defaults = LetteringSettings::default();
    assert_eq!(
        decide(None, true, None, &defaults).action,
        SfxAction::Translate
    );
    assert_eq!(
        decide(None, false, Some(true), &defaults).action,
        SfxAction::Skip
    );
    for (outside, expected) in [
        (SfxOutsideMode::Skip, SfxAction::Skip),
        (SfxOutsideMode::Subtitle, SfxAction::Subtitle),
        (SfxOutsideMode::Replace, SfxAction::Translate),
    ] {
        let mut settings = LetteringSettings {
            sfx_outside: outside,
            sfx_important_only: true,
            ..defaults.clone()
        };
        for importance in [None, Some(false)] {
            assert_eq!(
                decide(None, false, importance, &settings),
                EffectiveSfx {
                    action: SfxAction::Skip,
                    reason: SfxReason::Unimportant,
                    automatic: true,
                }
            );
            assert_eq!(
                decide(None, true, importance, &settings).action,
                SfxAction::Translate
            );
        }
        assert_eq!(decide(None, false, Some(true), &settings).action, expected);
        settings.sfx_important_only = false;
        assert_eq!(decide(None, false, None, &settings).action, expected);
        settings.sfx_inside = SfxInsideMode::Skip;
        assert_eq!(
            decide(None, true, Some(true), &settings).action,
            SfxAction::Skip
        );
    }
}

#[test]
fn explicit_box_choices_override_both_settings_and_unknown_importance() {
    for action in [SfxAction::Skip, SfxAction::Subtitle, SfxAction::Translate] {
        for inside in [false, true] {
            for importance in [None, Some(false), Some(true)] {
                let settings = LetteringSettings {
                    sfx_inside: SfxInsideMode::Skip,
                    sfx_outside: SfxOutsideMode::Replace,
                    sfx_important_only: true,
                    ..Default::default()
                };
                assert_eq!(
                    decide(Some(action), inside, importance, &settings),
                    EffectiveSfx {
                        action,
                        reason: SfxReason::User,
                        automatic: false,
                    }
                );
            }
        }
    }
}

async fn outside_sfx() -> (tempfile::TempDir, Project, EntityId) {
    let (directory, mut project, layers) = super::super::tests::project_fixture().await;
    let layer = layers[2];
    let patch = project
        .snapshot()
        .patch(|edit| edit.set(layer, &Geometry::rectangle(300., 100., 60., 100.)))
        .unwrap();
    project.commit(patch).await.unwrap();
    project.set_source_text(layer, "knock".into()).await.unwrap();
    project
        .update_context(|context| context.set_model_category(layer, LetteringCategory::Sfx, None))
        .unwrap();
    (directory, project, layer)
}

#[tokio::test]
async fn subtitle_choice_linkage_and_visibility_undo_together_without_claiming_the_font() {
    let (_directory, mut project, layer) = outside_sfx().await;
    let base_style = project
        .snapshot()
        .component::<Typography>(layer)
        .unwrap()
        .unwrap();
    let settings = LetteringSettings::default();
    let styles = TextStyles::default();
    let subtitle_settings = OcrBoxSettings::default();
    let (commit, subtitle) = apply(
        &mut project,
        layer,
        Some(Some(SfxAction::Subtitle)),
        true,
        &subtitle_settings,
        &styles,
        &settings,
    )
    .await
    .unwrap();
    let subtitle = subtitle.unwrap();
    project.record_commit(&commit.unwrap());
    let snapshot = project.snapshot();
    assert_eq!(project.undo.len(), 1);
    assert_eq!(subtitle_source(&snapshot, subtitle).unwrap(), Some(layer));
    assert_eq!(
        snapshot.component::<Typography>(layer).unwrap().unwrap().origin,
        base_style.origin
    );
    assert!(!snapshot.component::<Visibility>(layer).unwrap().unwrap().visible);
    let content = snapshot.text_layer(subtitle).unwrap().content().unwrap();
    assert_eq!(
        content.source().unwrap().unwrap().text.value,
        "knock"
    );
    assert_eq!(
        effective(&project.load_context().unwrap(), &snapshot, layer, &settings)
            .unwrap()
            .unwrap()
            .reason,
        SfxReason::User
    );

    project.undo().await.unwrap();
    assert!(project.snapshot().text_layer(subtitle).is_err());
    let snapshot = project.snapshot();
    assert_eq!(snapshot.component::<Typography>(layer).unwrap().unwrap(), base_style);
    assert!(snapshot.component::<Visibility>(layer).unwrap().is_none());
    assert!(
        effective(&project.load_context().unwrap(), &snapshot, layer, &settings)
            .unwrap()
            .unwrap()
            .automatic
    );
    project.redo().await.unwrap();
    assert_eq!(
        find_subtitle(&project.snapshot(), layer).unwrap(),
        Some(subtitle)
    );
    let revision = project.snapshot().revision();
    let (commit, repeated) = apply(
        &mut project,
        layer,
        None,
        true,
        &subtitle_settings,
        &styles,
        &settings,
    )
    .await
    .unwrap();
    assert!(commit.is_none());
    assert_eq!(repeated, Some(subtitle));
    assert_eq!(project.snapshot().revision(), revision);
}

#[tokio::test]
async fn rerunning_preserves_subtitle_edits_and_manual_non_sfx_category() {
    let (_directory, mut project, layer) = outside_sfx().await;
    let settings = LetteringSettings {
        sfx_outside: SfxOutsideMode::Subtitle,
        ..Default::default()
    };
    let styles = TextStyles::default();
    let subtitle_settings = OcrBoxSettings::default();
    let (_, subtitle) = apply(
        &mut project,
        layer,
        None,
        true,
        &subtitle_settings,
        &styles,
        &settings,
    )
    .await
    .unwrap();
    let subtitle = subtitle.unwrap();
    project
        .set_translation(subtitle, Some("tap tap".into()))
        .await
        .unwrap();
    let snapshot = project.snapshot();
    let mut hand_style = snapshot.component::<Typography>(subtitle).unwrap().unwrap();
    hand_style.size = Some(29.);
    let hand_geometry = Geometry::rectangle(400., 200., 150., 30.);
    let patch = snapshot
        .patch(|edit| {
            edit.set(subtitle, &hand_style)?;
            edit.set(subtitle, &hand_geometry)
        })
        .unwrap();
    project.commit(patch).await.unwrap();
    let (commit, repeated) = apply(
        &mut project,
        layer,
        None,
        true,
        &subtitle_settings,
        &styles,
        &settings,
    )
    .await
    .unwrap();
    assert!(commit.is_none());
    assert_eq!(repeated, Some(subtitle));
    let snapshot = project.snapshot();
    assert_eq!(
        snapshot.component::<Typography>(subtitle).unwrap().unwrap().size,
        Some(29.)
    );
    assert_eq!(
        snapshot.component::<Geometry>(subtitle).unwrap().unwrap(),
        hand_geometry
    );
    let content = snapshot.text_layer(subtitle).unwrap().content().unwrap();
    assert_eq!(content.translation().unwrap().unwrap().text.value, "tap tap");

    project
        .update_context(|context| {
            context.set_user_category(layer, Some(LetteringCategory::NormalDialogue))
        })
        .unwrap();
    apply(
        &mut project,
        layer,
        None,
        true,
        &subtitle_settings,
        &styles,
        &settings,
    )
    .await
    .unwrap();
    let snapshot = project.snapshot();
    assert!(
        effective(&project.load_context().unwrap(), &snapshot, layer, &settings)
            .unwrap()
            .is_none()
    );
    assert!(
        snapshot.component::<Visibility>(layer).unwrap()
            .map_or(true, |visibility| visibility.visible)
    );
    assert!(!snapshot.component::<Visibility>(subtitle).unwrap().unwrap().visible);
    let content = snapshot.text_layer(subtitle).unwrap().content().unwrap();
    assert_eq!(content.translation().unwrap().unwrap().text.value, "tap tap");
}

#[tokio::test]
async fn automatic_visibility_can_change_but_manual_visibility_wins() {
    let (_directory, mut project, layer) = outside_sfx().await;
    let mut settings = LetteringSettings::default();
    let styles = TextStyles::default();
    let subtitle_settings = OcrBoxSettings::default();
    apply(
        &mut project,
        layer,
        None,
        false,
        &subtitle_settings,
        &styles,
        &settings,
    )
    .await
    .unwrap();
    assert!(
        !project.snapshot().component::<Visibility>(layer).unwrap().unwrap().visible
    );
    settings.sfx_outside = SfxOutsideMode::Replace;
    apply(
        &mut project,
        layer,
        None,
        false,
        &subtitle_settings,
        &styles,
        &settings,
    )
    .await
    .unwrap();
    assert!(
        project.snapshot().component::<Visibility>(layer).unwrap()
            .map_or(true, |visibility| visibility.visible)
    );
    let patch = project.snapshot()
        .patch(|edit| edit.set(layer, &Visibility {
            origin: Origin::User,
            visible: false,
            opacity: 0.7,
        }))
        .unwrap();
    project.commit(patch).await.unwrap();
    apply(
        &mut project,
        layer,
        None,
        false,
        &subtitle_settings,
        &styles,
        &settings,
    )
    .await
    .unwrap();
    let visibility = project
        .snapshot()
        .component::<Visibility>(layer)
        .unwrap()
        .unwrap();
    assert!(!visibility.visible);
    assert_eq!(visibility.opacity, 0.7);
}
