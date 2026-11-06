use booharu_context::LetteringCategory;
use koharu_renderer::{SizePart, TextStyles};
use koharu_scene::{
    At, Authored, FitsTo, FlowsIn, Geometry, Origin, PageDraft, SourceText, Translation,
};

use super::*;
use crate::commands::project::Project;

async fn project() -> (Project, EntityId) {
    let mut session = koharu_scene::Session::memory().await.unwrap();
    let mut page = None;
    let patch = session
        .snapshot()
        .patch(|edit| {
            page = Some(edit.add_page(PageDraft::new("page", 500., 500.), At::End)?);
            Ok(())
        })
        .unwrap();
    session.commit(patch).await.unwrap();
    (Project::new(session, "OCR".into()), page.unwrap())
}

#[tokio::test]
async fn creates_a_readable_detected_style_box_in_one_undo_step() {
    let (mut project, page) = project().await;
    let base = project.snapshot();
    let mut styles = TextStyles::default();
    styles.default_style.size = Some(SizePart {
        auto_fit: false,
        size: Some(24.),
    });
    let (commit, layer) = project
        .create_ocr_box(
            page,
            Bounds {
                x: 10.,
                y: 20.,
                width: 30.,
                height: 60.,
            },
            &styles,
        )
        .await
        .unwrap();
    project.record_commit(&commit);
    let snapshot = project.snapshot();
    let text = snapshot.text_layer(layer).unwrap();
    assert!(text.content().unwrap().source().unwrap().is_none());
    assert!(text.content().unwrap().source_region().unwrap().is_some());
    assert!(snapshot.relation_from::<FitsTo>(layer).unwrap().is_some());
    assert_eq!(text.typography().unwrap().unwrap().size, Some(24.));
    assert_eq!(project.undo.len(), 1);
    project.undo().await.unwrap();
    assert!(project.snapshot().text_layer(layer).is_err());
    assert_eq!(project.snapshot().pages().count(), base.pages().count());
}

#[tokio::test]
async fn ocr_box_uses_the_smallest_bubble_without_changing_its_drawn_frame() {
    use koharu_scene::BubbleRegion;
    let (mut project, page) = project().await;
    let mut small = None;
    let patch = project
        .snapshot()
        .patch(|edit| {
            edit.add_analysis_region::<BubbleRegion>(
                page,
                At::End,
                &Geometry::rectangle(0., 0., 400., 400.),
                None,
            )?;
            small = Some(edit.add_analysis_region::<BubbleRegion>(
                page,
                At::End,
                &Geometry::rectangle(10., 10., 80., 100.),
                None,
            )?);
            Ok(())
        })
        .unwrap();
    project.commit(patch).await.unwrap();
    let (_, layer) = project
        .create_ocr_box(
            page,
            Bounds {
                x: 20.,
                y: 20.,
                width: 30.,
                height: 60.,
            },
            &TextStyles::default(),
        )
        .await
        .unwrap();
    let snapshot = project.snapshot();
    assert_eq!(
        snapshot
            .relation_from::<FlowsIn>(layer)
            .unwrap()
            .unwrap()
            .value()
            .target,
        small.unwrap()
    );
    assert_eq!(
        snapshot.component::<Geometry>(layer).unwrap().unwrap(),
        Geometry::rectangle(20., 20., 30., 60.)
    );
}

#[tokio::test]
async fn redo_requires_permission_and_clears_only_the_selected_field() {
    let (mut project, page) = project().await;
    let (_, layer) = project
        .create_ocr_box(
            page,
            Bounds {
                x: 10.,
                y: 20.,
                width: 30.,
                height: 60.,
            },
            &TextStyles::default(),
        )
        .await
        .unwrap();
    project
        .set_source_text(layer, "manual source".into())
        .await
        .unwrap();
    project
        .set_translation(layer, Some("manual translation".into()))
        .await
        .unwrap();
    let before = project.revision();
    assert!(
        project
            .clear_redo_edits(&[layer], RedoAction::Read, false)
            .await
            .is_err()
    );
    assert_eq!(project.revision(), before);
    assert!(
        project
            .clear_redo_edits(&[layer], RedoAction::Read, true)
            .await
            .unwrap()
            .is_some()
    );
    let snapshot = project.snapshot();
    let content = snapshot.text_layer(layer).unwrap().content().unwrap();
    let source = content.source().unwrap().unwrap();
    assert!(source.text.value.is_empty());
    assert_ne!(source.text.origin, Origin::User);
    assert_eq!(
        content.translation().unwrap().unwrap().text.value,
        "manual translation"
    );
    assert!(
        project
            .clear_redo_edits(&[layer], RedoAction::Translate, false)
            .await
            .is_err()
    );
    project
        .clear_redo_edits(&[layer], RedoAction::Translate, true)
        .await
        .unwrap();
    assert!(
        project
            .snapshot()
            .text_layer(layer)
            .unwrap()
            .content()
            .unwrap()
            .translation()
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn text_tool_does_not_mark_an_untouched_source_as_a_hand_edit() {
    let (mut project, page) = project().await;
    let (_, layer) = project
        .add_text_box(
            page,
            koharu_desktop::Frame {
                x: 10.,
                y: 20.,
                width: 30.,
                height: 60.,
                angle_degrees: 0.,
            },
        )
        .await
        .unwrap();
    project
        .set_translation(layer, Some("typed translation".into()))
        .await
        .unwrap();
    let snapshot = project.snapshot();
    let content = snapshot.text_layer(layer).unwrap().content().unwrap();
    let source = content.source().unwrap().unwrap();
    assert!(source.text.value.is_empty());
    assert_ne!(source.text.origin, Origin::User);
    assert!(content.source_region().unwrap().is_none());
    assert_eq!(
        content.translation().unwrap().unwrap().text.origin,
        Origin::User
    );
}

#[tokio::test]
async fn sfx_subtitle_copies_text_is_small_text_and_undoes_as_one_creation() {
    let (mut project, page) = project().await;
    let directory = tempfile::tempdir().unwrap();
    project.directory = Some(directory.path().to_owned());
    let (_, layer) = project
        .create_ocr_box(
            page,
            Bounds {
                x: 300.,
                y: 20.,
                width: 30.,
                height: 100.,
            },
            &TextStyles::default(),
        )
        .await
        .unwrap();
    project
        .update_context(|context| context.set_user_category(layer, Some(LetteringCategory::Sfx)))
        .unwrap();
    let content = project
        .snapshot()
        .text_layer(layer)
        .unwrap()
        .content()
        .unwrap()
        .id();
    let patch = project
        .snapshot()
        .patch(|edit| {
            edit.set(
                content,
                &SourceText {
                    text: Authored::user("ドン".into()),
                    language: None,
                },
            )?;
            edit.set(
                content,
                &Translation {
                    text: Authored::user("THUD".into()),
                    language: None,
                },
            )
        })
        .unwrap();
    project.commit(patch).await.unwrap();
    let (commit, added, translate) = project
        .create_sfx_subtitle(layer, &OcrBoxSettings::default(), &TextStyles::default())
        .await
        .unwrap();
    project.record_commit(&commit.unwrap());
    assert!(!translate);
    let snapshot = project.snapshot();
    let subtitle = snapshot.text_layer(added).unwrap();
    let source_region = snapshot
        .text_layer(layer)
        .unwrap()
        .content()
        .unwrap()
        .source_region()
        .unwrap()
        .unwrap();
    let subtitle_region = subtitle.content().unwrap().source_region().unwrap().unwrap();
    assert_ne!(subtitle_region.id(), source_region.id());
    assert_eq!(
        subtitle_region.geometry().unwrap(),
        source_region.geometry().unwrap()
    );
    assert_eq!(
        snapshot.parent(subtitle_region.id()).unwrap(),
        Some(subtitle.content().unwrap().id())
    );
    assert_eq!(
        subtitle
            .content()
            .unwrap()
            .source()
            .unwrap()
            .unwrap()
            .text
            .value,
        "ドン"
    );
    assert_eq!(
        subtitle
            .content()
            .unwrap()
            .translation()
            .unwrap()
            .unwrap()
            .text
            .value,
        "THUD"
    );
    assert_eq!(
        project
            .load_context()
            .unwrap()
            .category(added)
            .unwrap()
            .value,
        LetteringCategory::SmallText
    );
    let skip =
        crate::commands::lettering::translation_skip(&project.load_context().unwrap(), &snapshot)
            .unwrap();
    assert!(!skip.contains(&added));
    let style = subtitle.typography().unwrap().unwrap();
    assert_eq!(style.size, Some(18.));
    assert!(!style.auto_fit);
    assert_eq!(style.color, Some([0, 0, 0, 255]));
    assert_eq!(style.stroke_color, Some([255; 4]));
    assert_eq!(
        subtitle
            .content()
            .unwrap()
            .translation()
            .unwrap()
            .unwrap()
            .text
            .origin,
        Origin::User
    );
    assert!(subtitle.balloon_target().unwrap().is_none());
    assert!(subtitle.fit_target().unwrap().is_none());
    assert!(!project.box_action_info(added).unwrap().can_split);
    assert!(project.split_box(added).await.is_err());
    assert!(
        snapshot
            .asset(page, &koharu_scene::AssetRole::new("text-mask").unwrap())
            .unwrap()
            .is_none()
    );
    let revision = project.revision();
    let (duplicate, existing, translate) = project
        .create_sfx_subtitle(layer, &OcrBoxSettings::default(), &TextStyles::default())
        .await
        .unwrap();
    assert!(duplicate.is_none());
    assert_eq!(existing, added);
    assert!(!translate);
    assert_eq!(project.revision(), revision);
    assert_eq!(project.undo.len(), 1);
    project.undo().await.unwrap();
    assert!(project.snapshot().text_layer(added).is_err());
    assert!(project.snapshot().entity(subtitle_region.id()).is_err());
    assert!(project.snapshot().text_layer(layer).is_ok());
    project.redo().await.unwrap();
    let (duplicate, existing, translate) = project
        .create_sfx_subtitle(layer, &OcrBoxSettings::default(), &TextStyles::default())
        .await
        .unwrap();
    assert!(duplicate.is_none());
    assert_eq!(existing, added);
    assert!(!translate);
}

#[tokio::test]
async fn subtitle_link_survives_style_edits_and_duplicate_requests_preserve_hand_edits() {
    use crate::commands::{editing::TypographyUpdate, project::Layer};
    let (mut project, page) = project().await;
    let directory = tempfile::tempdir().unwrap();
    project.directory = Some(directory.path().to_owned());
    let (_, source) = project
        .create_ocr_box(
            page,
            Bounds {
                x: 10.,
                y: 20.,
                width: 80.,
                height: 30.,
            },
            &TextStyles::default(),
        )
        .await
        .unwrap();
    project.set_source_text(source, "SFX".into()).await.unwrap();
    project
        .update_context(|context| context.set_user_category(source, Some(LetteringCategory::Sfx)))
        .unwrap();
    let (_, added, _) = project
        .create_sfx_subtitle(source, &OcrBoxSettings::default(), &TextStyles::default())
        .await
        .unwrap();
    let mut typography = Project::page(&project.snapshot(), page)
        .unwrap()
        .layers
        .into_iter()
        .find_map(|layer| match layer {
            Layer::Text { id, typography, .. } if id == added => typography,
            _ => None,
        })
        .unwrap();
    typography.size = Some(27.);
    project
        .set_typography(vec![TypographyUpdate {
            layer: added,
            typography,
        }])
        .await
        .unwrap();
    project
        .set_translation(added, Some("My subtitle".into()))
        .await
        .unwrap();
    let frame = Geometry::rectangle(100., 200., 200., 60.);
    project
        .set_geometries([(added, frame.clone(), 0.)])
        .await
        .unwrap();
    let revision = project.revision();
    let (commit, existing, translate) = project
        .create_sfx_subtitle(source, &OcrBoxSettings::default(), &TextStyles::default())
        .await
        .unwrap();
    assert!(commit.is_none());
    assert_eq!(existing, added);
    assert!(!translate);
    assert_eq!(project.revision(), revision);
    let snapshot = project.snapshot();
    let subtitle = snapshot.text_layer(added).unwrap();
    let region = subtitle
        .content()
        .unwrap()
        .source_region()
        .unwrap()
        .unwrap()
        .id();
    assert_eq!(subtitle.typography().unwrap().unwrap().size, Some(27.));
    assert_eq!(subtitle.frame().unwrap().unwrap(), frame);
    assert_eq!(
        subtitle
            .content()
            .unwrap()
            .translation()
            .unwrap()
            .unwrap()
            .text
            .value,
        "My subtitle"
    );

    let deletion = project.delete_layers(vec![added]).await.unwrap();
    project.record_commit(&deletion);
    assert!(project.snapshot().entity(region).is_err());
    project.undo().await.unwrap();
    assert!(project.snapshot().entity(region).is_ok());
    assert_eq!(
        project
            .create_sfx_subtitle(source, &OcrBoxSettings::default(), &TextStyles::default())
            .await
            .unwrap()
            .1,
        added
    );
    project.redo().await.unwrap();
    let (commit, replacement, _) = project
        .create_sfx_subtitle(source, &OcrBoxSettings::default(), &TextStyles::default())
        .await
        .unwrap();
    assert!(commit.is_some());
    assert_ne!(replacement, added);
}

#[tokio::test]
async fn subtitle_cleanup_uses_original_artwork_after_the_sfx_moves_or_is_deleted() {
    let (mut project, page) = project().await;
    let directory = tempfile::tempdir().unwrap();
    project.directory = Some(directory.path().to_owned());
    let (_, source) = project
        .create_ocr_box(
            page,
            Bounds {
                x: 10.,
                y: 20.,
                width: 80.,
                height: 30.,
            },
            &TextStyles::default(),
        )
        .await
        .unwrap();
    project.set_source_text(source, "SFX".into()).await.unwrap();
    project
        .update_context(|context| context.set_user_category(source, Some(LetteringCategory::Sfx)))
        .unwrap();
    let (_, added, _) = project
        .create_sfx_subtitle(source, &OcrBoxSettings::default(), &TextStyles::default())
        .await
        .unwrap();
    let subtitle_frame = project
        .snapshot()
        .text_layer(added)
        .unwrap()
        .frame()
        .unwrap()
        .unwrap();
    let original = Geometry::rectangle(10., 20., 80., 30.);
    project
        .set_geometries([(source, Geometry::rectangle(300., 300., 80., 30.), 0.)])
        .await
        .unwrap();
    assert_eq!(box_geometry(&project.snapshot(), added, true).unwrap(), original);
    assert_eq!(
        box_geometry(&project.snapshot(), added, false).unwrap(),
        subtitle_frame
    );
    let commit = project.delete_layers(vec![source]).await.unwrap();
    project.record_commit(&commit);
    assert!(project.snapshot().text_layer(added).is_ok());
    assert_eq!(box_geometry(&project.snapshot(), added, true).unwrap(), original);
    project.undo().await.unwrap();
    assert_eq!(box_geometry(&project.snapshot(), added, true).unwrap(), original);
}

#[tokio::test]
async fn empty_untranslated_sfx_does_not_create_an_invisible_subtitle() {
    let (mut project, page) = project().await;
    let directory = tempfile::tempdir().unwrap();
    project.directory = Some(directory.path().to_owned());
    let (_, source) = project
        .create_ocr_box(
            page,
            Bounds {
                x: 10.,
                y: 20.,
                width: 80.,
                height: 30.,
            },
            &TextStyles::default(),
        )
        .await
        .unwrap();
    project
        .update_context(|context| context.set_user_category(source, Some(LetteringCategory::Sfx)))
        .unwrap();
    let revision = project.revision();
    assert!(
        project
            .create_sfx_subtitle(source, &OcrBoxSettings::default(), &TextStyles::default())
            .await
            .is_err()
    );
    assert_eq!(project.revision(), revision);
    assert_eq!(
        project
            .snapshot()
            .page(page)
            .unwrap()
            .text_layers()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn untranslated_sfx_subtitle_requests_its_own_translation() {
    let (mut project, page) = project().await;
    let directory = tempfile::tempdir().unwrap();
    project.directory = Some(directory.path().to_owned());
    let (_, layer) = project
        .create_ocr_box(
            page,
            Bounds {
                x: 10.,
                y: 20.,
                width: 80.,
                height: 30.,
            },
            &TextStyles::default(),
        )
        .await
        .unwrap();
    project.set_source_text(layer, "ドン".into()).await.unwrap();
    project
        .update_context(|context| context.set_user_category(layer, Some(LetteringCategory::Sfx)))
        .unwrap();
    let (_, added, translate) = project
        .create_sfx_subtitle(layer, &OcrBoxSettings::default(), &TextStyles::default())
        .await
        .unwrap();
    assert!(translate);
    assert!(
        project
            .snapshot()
            .text_layer(added)
            .unwrap()
            .content()
            .unwrap()
            .translation()
            .unwrap()
            .is_none()
    );
    let revision = project.revision();
    let (duplicate, existing, translate) = project
        .create_sfx_subtitle(layer, &OcrBoxSettings::default(), &TextStyles::default())
        .await
        .unwrap();
    assert!(duplicate.is_none());
    assert_eq!(existing, added);
    assert!(!translate);
    assert_eq!(project.revision(), revision);
}
