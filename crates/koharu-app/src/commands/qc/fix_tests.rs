use super::*;
use koharu_scene::{At, Authored, Generation, PageDraft, Point, ProducerId, TextLayoutKind};

async fn generated_box() -> (tempfile::TempDir, Project, EntityId, EntityId, Generation) {
    let directory = tempfile::tempdir().unwrap();
    let mut project = Project::create("QC repair".into(), directory.path().join("chapter.khrproj"))
        .await
        .unwrap();
    let generation = Generation::new(ProducerId::new("dev.koharu.pipeline.detection").unwrap());
    let mut edit = project.snapshot().edit_as(generation.clone());
    let page = edit
        .add_page(PageDraft::new("page", 100.0, 100.0), At::End)
        .unwrap();
    let content = edit.add_text_content(page, At::End).unwrap();
    edit.set(
        content,
        &SourceText {
            text: Authored::generated("source".into(), generation.clone()),
            language: None,
        },
    )
    .unwrap();
    edit.set(
        content,
        &Translation {
            text: Authored::generated("Earlier wording".into(), generation.clone()),
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
    edit.set(layer, &Geometry::rectangle(10.0, 10.0, 20.0, 20.0))
        .unwrap();
    edit.set(
        layer,
        &Typography {
            origin: Origin::Generated(generation.clone()),
            preferred_font: None,
            font_weight: None,
            font_style: None,
            size: None,
            auto_fit: true,
            color: None,
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
    project.commit(edit.finish().unwrap()).await.unwrap();
    (directory, project, page, layer, generation)
}

#[tokio::test]
async fn previous_wording_and_fix_history_follow_the_same_undo_step() {
    let (_directory, mut project, _page, layer, generation) = generated_box().await;
    let snapshot = project.snapshot();
    let content = snapshot.text_layer(layer).unwrap().content().unwrap().id();
    let record = QcAutoFix {
        reason: QcCheck::Overflow,
        action: QcFixAction::ShortenTranslation,
        before: before(&snapshot, layer).unwrap(),
    };
    let mut edit = snapshot.edit_as(generation);
    let mut translation = snapshot.component::<Translation>(content).unwrap().unwrap();
    translation.text.value = "Shorter".into();
    edit.set(content, &translation).unwrap();
    let changed = project.commit(edit.finish().unwrap()).await.unwrap();
    let current = project.snapshot();
    let mut typography = current.component::<Typography>(layer).unwrap().unwrap();
    typography
        .extensions
        .insert(HISTORY.into(), serde_json::to_string(&vec![record]).unwrap());
    let marker = current
        .patch(|edit| edit.set_typography_preserving_origin(layer, &typography))
        .unwrap();
    let marked = project.commit(marker).await.unwrap();
    project.record(vec![changed.revision, marked.revision]);

    assert_eq!(project.undo.len(), 1);
    let history = auto_fixes(&project.snapshot(), layer).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        history[0].before.translation.as_ref().unwrap().text.value,
        "Earlier wording"
    );
    assert!(
        !before(&project.snapshot(), layer)
            .unwrap()
            .typography
            .unwrap()
            .extensions
            .contains_key(HISTORY)
    );
    assert!(
        !hand_edited(&project.snapshot(), &project.load_context().unwrap(), layer).unwrap()
    );
    project.undo().await.unwrap();
    assert!(auto_fixes(&project.snapshot(), layer).unwrap().is_empty());
    assert_eq!(
        project.snapshot().component::<Translation>(content).unwrap().unwrap().text.value,
        "Earlier wording"
    );
    project.redo().await.unwrap();
    assert_eq!(auto_fixes(&project.snapshot(), layer).unwrap().len(), 1);
    assert_eq!(
        project.snapshot().component::<Translation>(content).unwrap().unwrap().text.value,
        "Shorter"
    );
}

#[tokio::test]
async fn shared_content_is_not_an_isolated_model_target_and_manual_styles_are_protected() {
    let (_directory, mut project, page, layer, generation) = generated_box().await;
    let snapshot = project.snapshot();
    assert!(isolated_model_target(&snapshot, layer).unwrap());
    assert!(!hand_edited(&snapshot, &project.load_context().unwrap(), layer).unwrap());
    let content = snapshot.text_layer(layer).unwrap().content().unwrap().id();
    let layout = snapshot.component::<TextLayout>(layer).unwrap().unwrap();
    let mut edit = snapshot.edit_as(generation);
    edit.add_text_layer(page, At::End, content, &layout).unwrap();
    project.commit(edit.finish().unwrap()).await.unwrap();
    assert!(!isolated_model_target(&project.snapshot(), layer).unwrap());
    let snapshot = project.snapshot();
    let mut typography = snapshot.component::<Typography>(layer).unwrap().unwrap();
    typography.preferred_font = Some("Hand chosen".into());
    let patch = snapshot.patch(|edit| edit.set(layer, &typography)).unwrap();
    project.commit(patch).await.unwrap();
    assert!(
        hand_edited(&project.snapshot(), &project.load_context().unwrap(), layer).unwrap()
    );
}

#[test]
fn widening_cannot_bridge_a_concave_bubble_or_rewrite_a_rotated_frame() {
    let boundary = Geometry::rectangle(0.0, 0.0, 20.0, 20.0);
    let frame = Geometry::rectangle(5.0, 5.0, 5.0, 5.0);
    let widened = widen_inside(&frame, &boundary).unwrap();
    assert!(widened.points[0].x < frame.points[0].x);
    assert!(polygon_contains_rectangle(&boundary.points, &widened.points));

    let concave = [
        (0.0, 0.0), (10.0, 0.0), (10.0, 4.0), (6.0, 4.0),
        (6.0, 6.0), (10.0, 6.0), (10.0, 10.0), (0.0, 10.0),
    ]
    .map(|(x, y)| Point { x, y });
    let crossing = Geometry::rectangle(5.0, 3.0, 4.0, 4.0);
    assert!(!polygon_contains_rectangle(&concave, &crossing.points));
    let rotated = Geometry {
        origin: Origin::User,
        points: [(5.0, 10.0), (10.0, 5.0), (15.0, 10.0), (10.0, 15.0)]
            .map(|(x, y)| Point { x, y })
            .into(),
    };
    assert!(widen_inside(&rotated, &boundary).is_none());
}
