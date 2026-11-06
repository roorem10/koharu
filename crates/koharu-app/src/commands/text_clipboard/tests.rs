use super::*;
use koharu_scene::{
    EntityOrigin, FitsTo, FontStyle, Generation, LanguageTag, Origin, PageDraft, ProducerId,
    RecognizedFrom, RunStyle, Session, StrokeJoin, StrokeMethod, StrokePosition, StyleRun,
    TextAlignment, TextLayoutKind, TextRegion, TextWarp, WritingMode,
};

fn generation() -> Generation {
    Generation::new(ProducerId::new("dev.koharu.test.clipboard").unwrap())
}

async fn fixture() -> (Project, EntityId, EntityId, Vec<EntityId>) {
    let mut session = Session::memory().await.unwrap();
    let mut edit = session.snapshot().edit();
    let source = edit
        .add_page(PageDraft::new("source", 200.0, 100.0), At::End)
        .unwrap();
    let destination = edit
        .add_page(PageDraft::new("destination", 100.0, 80.0), At::End)
        .unwrap();
    session.commit(edit.finish().unwrap()).await.unwrap();
    let mut edit = session.snapshot().edit_as(generation());
    let region = edit
        .add_analysis_region::<TextRegion>(
            source,
            At::End,
            &Geometry::rectangle(150.0, 60.0, 30.0, 20.0),
            None,
        )
        .unwrap();
    let mut layers = Vec::new();
    for index in 0..2 {
        let content = edit.add_text_content(source, At::End).unwrap();
        edit.set(
            content,
            &SourceText {
                text: Authored::user("ドン！".into()),
                language: Some(LanguageTag::new("ja").unwrap()),
            },
        )
        .unwrap();
        edit.set(
            content,
            &Translation {
                text: Authored::user("BOOM!".into()),
                language: Some(LanguageTag::new("en").unwrap()),
            },
        )
        .unwrap();
        edit.set(
            content,
            &TextRole {
                origin: Origin::User,
                role: "dev.koharu.role.sfx".into(),
            },
        )
        .unwrap();
        edit.relate::<RecognizedFrom>(content, region).unwrap();
        let layer = edit
            .add_text_layer(
                source,
                At::End,
                content,
                &TextLayout {
                    origin: Origin::User,
                    kind: TextLayoutKind::Paragraph,
                    angle_degrees: Some(15.0),
                    warp: Some(TextWarp::Skew {
                        x_degrees: 8.0,
                        y_degrees: -3.0,
                    }),
                },
            )
            .unwrap();
        edit.relate::<FitsTo>(layer, region).unwrap();
        edit.set(
            layer,
            &Typography {
                origin: Origin::User,
                preferred_font: Some(format!("Font {index}")),
                font_weight: Some(700),
                font_style: Some(FontStyle::Italic),
                size: Some(24.0),
                auto_fit: false,
                color: Some([90, 30, 210, 255]),
                stroke_color: Some([255, 255, 255, 255]),
                stroke_width: Some(2.0),
                stroke_method: Some(StrokeMethod::Contour),
                stroke_position: Some(StrokePosition::Outside),
                stroke_join: Some(StrokeJoin::Round),
                outer_stroke_color: Some([0, 0, 0, 255]),
                outer_stroke_width: Some(4.0),
                line_spacing: Some(1.2),
                letter_spacing: Some(20.0),
                fill_gradient: None,
                alignment: Some(TextAlignment::Center),
                writing_mode: Some(WritingMode::Horizontal),
                extensions: [("dev.booharu.test.effect".into(), "retained".into())].into(),
            },
        )
        .unwrap();
        edit.set(
            layer,
            &TextRuns {
                origin: Origin::User,
                runs: vec![StyleRun {
                    start: 1,
                    end: 4,
                    style: RunStyle {
                        font: Some("Mixed font".into()),
                        size_scale: Some(1.5),
                        color: Some([255, 10, 0, 255]),
                        outer_stroke_color: Some([0, 255, 0, 255]),
                        outer_stroke_width: Some(3.0),
                        ..Default::default()
                    },
                }],
            },
        )
        .unwrap();
        edit.set(
            layer,
            &Visibility {
                origin: Origin::User,
                visible: true,
                opacity: 0.7,
            },
        )
        .unwrap();
        layers.push(layer);
    }
    session.commit(edit.finish().unwrap()).await.unwrap();
    (
        Project::new(session, "clipboard".into()),
        source,
        destination,
        layers,
    )
}

#[tokio::test]
async fn payload_round_trip_preserves_rich_text_and_detaches_analysis_in_one_undo_step() {
    let (mut project, _, destination, layers) = fixture().await;
    let captured = TextBoxes::capture(
        &project.snapshot(),
        &[layers[1], layers[0], layers[1]],
        &Renderer::default(),
    )
    .await
    .unwrap();
    let payload: TextBoxes = serde_json::from_str(&serde_json::to_string(&captured).unwrap()).unwrap();
    assert_eq!(payload, captured);
    assert_eq!(payload.boxes.len(), 2);
    assert_eq!(
        payload.boxes[0]
            .typography
            .as_ref()
            .unwrap()
            .preferred_font
            .as_deref(),
        Some("Font 0"),
    );
    let (commit, pasted) = project
        .paste_text_boxes(&payload, destination, 10.0)
        .await
        .unwrap();
    project.record_commit(&commit);
    assert_eq!(project.undo.len(), 1);
    let snapshot = project.snapshot();
    for (&id, original) in pasted.iter().zip(&payload.boxes) {
        let layer = snapshot.text_layer(id).unwrap();
        let content = layer.content().unwrap();
        assert!(!layers.contains(&id));
        assert!(content.source_region().unwrap().is_none());
        assert!(layer.automatic_target().unwrap().is_none());
        let mut expected = original.clone();
        expected.source.text.origin = Origin::User;
        expected.translation.text.origin = Origin::User;
        expected.layout.origin = Origin::User;
        expected.typography.as_mut().unwrap().origin = Origin::User;
        expected.runs.as_mut().unwrap().origin = Origin::User;
        expected.role.as_mut().unwrap().origin = Origin::User;
        expected.visibility.as_mut().unwrap().origin = Origin::User;
        assert_eq!(content.source().unwrap(), Some(expected.source));
        assert_eq!(content.translation().unwrap(), Some(expected.translation));
        assert_eq!(content.role().unwrap(), expected.role);
        assert_eq!(layer.layout().unwrap(), expected.layout);
        assert_eq!(layer.typography().unwrap(), expected.typography);
        assert_eq!(snapshot.component::<TextRuns>(id).unwrap(), expected.runs);
        assert_eq!(layer.visibility().unwrap(), expected.visibility);
        assert_eq!(
            layer.frame().unwrap(),
            Some(Geometry::rectangle(70.0, 60.0, 30.0, 20.0)),
        );
        for entity in [id, content.id()] {
            assert_eq!(
                snapshot
                    .component::<EntityOrigin>(entity)
                    .unwrap()
                    .unwrap()
                    .origin,
                Origin::User,
            );
        }
    }
    project.undo().await.unwrap();
    for &layer in &pasted {
        assert!(project.snapshot().entity(layer).is_err());
    }
    project.redo().await.unwrap();
    for &layer in &pasted {
        assert!(project.snapshot().text_layer(layer).is_ok());
    }
}

#[cfg(target_os = "windows")]
#[tokio::test]
async fn shared_bubble_copy_freezes_each_rendered_flow_instead_of_the_whole_bubble() {
    use koharu_scene::{BubbleRegion, FlowsIn};

    let (mut project, source, _, layers) = fixture().await;
    let renderer = Renderer::default();
    // Windows supplies Arial; font enumeration also fetches the remote catalog.
    let font = "Arial";
    let snapshot = project.snapshot();
    let bubble_geometry = Geometry::rectangle(10.0, 10.0, 180.0, 80.0);
    let patch = snapshot
        .patch(|edit| {
            let bubble = edit.add_analysis_region::<BubbleRegion>(
                source,
                At::End,
                &bubble_geometry,
                None,
            )?;
            for (index, &layer) in layers.iter().enumerate() {
                let content = snapshot.text_layer(layer)?.content()?.id();
                let original = snapshot.relation_from::<RecognizedFrom>(content)?.unwrap();
                edit.remove_relation(original.id())?;
                let region = edit.add_analysis_region::<TextRegion>(
                    source,
                    At::End,
                    &Geometry::rectangle(40.0 + index as f64 * 80.0, 30.0, 20.0, 40.0),
                    None,
                )?;
                edit.relate::<RecognizedFrom>(content, region)?;
                let fit = snapshot.relation_from::<FitsTo>(layer)?.unwrap();
                edit.remove_relation(fit.id())?;
                edit.relate::<FlowsIn>(layer, bubble)?;
                let mut typography = snapshot.component::<Typography>(layer)?.unwrap();
                typography.preferred_font = Some(font.into());
                edit.set(layer, &typography)?;
                edit.remove::<TextRuns>(layer)?;
                edit.set(
                    layer,
                    &TextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: None,
                        warp: None,
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    project.commit(patch).await.unwrap();
    let snapshot = project.snapshot();
    let rendered = renderer.render(&snapshot, source).await.unwrap();
    let payload = TextBoxes::capture(&snapshot, &layers, &renderer)
        .await
        .unwrap();
    for (&layer, copied) in layers.iter().zip(&payload.boxes) {
        let prepared = rendered.layer(layer).unwrap();
        assert_ne!(prepared.geometry().points, bubble_geometry.points);
        assert_eq!(copied.geometry.as_ref(), Some(prepared.geometry()));
        let LayerKind::Text(text) = prepared.kind() else {
            panic!("expected a text layer");
        };
        assert_eq!(copied.layout.angle_degrees, Some(text.angle_degrees));
    }
}

#[tokio::test]
async fn clipboard_survives_source_deletion_and_paste_is_independent() {
    let (mut project, source, destination, layers) = fixture().await;
    let payload = TextBoxes::capture(&project.snapshot(), &layers, &Renderer::default())
        .await
        .unwrap();
    let cut = project.delete_layers(layers.clone()).await.unwrap();
    project.record_commit(&cut);
    assert!(
        project
            .snapshot()
            .page(source)
            .unwrap()
            .text_layers()
            .unwrap()
            .next()
            .is_none()
    );
    let (commit, pasted) = project
        .paste_text_boxes(&payload, destination, 10.0)
        .await
        .unwrap();
    project.record_commit(&commit);
    assert_eq!(project.undo.len(), 2);
    project.undo().await.unwrap();
    assert!(project.snapshot().entity(pasted[0]).is_err());
    project.undo().await.unwrap();
    assert!(project.snapshot().entity(layers[0]).is_ok());
}

#[tokio::test]
async fn blank_copies_are_owned_even_when_the_source_has_no_text_records() {
    let (mut project, _, destination, layers) = fixture().await;
    let snapshot = project.snapshot();
    let original = snapshot.text_layer(layers[0]).unwrap().content().unwrap().id();
    let patch = snapshot
        .patch(|edit| {
            edit.remove::<Translation>(original)?;
            edit.remove::<SourceText>(original)?;
            edit.remove::<TextRuns>(layers[0])?;
            Ok(())
        })
        .unwrap();
    project.commit(patch).await.unwrap();
    let (_, pasted) = project
        .copy_text_boxes(&[layers[0]], destination, &Renderer::default())
        .await
        .unwrap();
    let snapshot = project.snapshot();
    let layer = snapshot.text_layer(pasted[0]).unwrap();
    let content = layer.content().unwrap();
    let source = content.source().unwrap().unwrap();
    let translation = content.translation().unwrap().unwrap();
    assert_eq!(source.text, Authored::user(String::new()));
    assert_eq!(translation.text, Authored::user(String::new()));
    let mut automatic = snapshot.edit_as(generation());
    assert!(automatic.set(content.id(), &source).is_err());
    assert!(automatic.set(content.id(), &translation).is_err());
    assert!(
        automatic
            .set(pasted[0], &layer.typography().unwrap().unwrap())
            .is_err()
    );
    assert!(automatic.set(pasted[0], &layer.layout().unwrap()).is_err());
}

#[tokio::test]
async fn offset_applies_only_to_the_source_page() {
    let (mut project, source, destination, layers) = fixture().await;
    let (_, same_page) = project
        .copy_text_boxes(&[layers[0]], source, &Renderer::default())
        .await
        .unwrap();
    assert_eq!(
        project.snapshot().text_layer(same_page[0]).unwrap().frame().unwrap(),
        Some(Geometry::rectangle(160.0, 70.0, 30.0, 20.0)),
    );
    let (_, other_page) = project
        .copy_text_boxes(&[layers[0]], destination, &Renderer::default())
        .await
        .unwrap();
    assert_eq!(
        project.snapshot().text_layer(other_page[0]).unwrap().frame().unwrap(),
        Some(Geometry::rectangle(70.0, 60.0, 30.0, 20.0)),
    );
}

#[test]
fn clamping_moves_polygons_without_resizing_them() {
    let geometry = Geometry::rectangle(-15.0, 90.0, 40.0, 30.0);
    assert_eq!(
        clamp_geometry(&geometry, 100.0, 100.0, 0.0),
        Geometry::rectangle(0.0, 70.0, 40.0, 30.0),
    );
    let oversized = Geometry::rectangle(25.0, 35.0, 200.0, 150.0);
    assert_eq!(
        clamp_geometry(&oversized, 100.0, 80.0, 10.0),
        Geometry::rectangle(0.0, 0.0, 200.0, 150.0),
    );
    let mut rotated = Geometry::rectangle(20.0, 20.0, 30.0, 40.0);
    rotated.points[0].x = -10.0;
    let moved = clamp_geometry(&rotated, 100.0, 100.0, 0.0);
    for (before, after) in rotated.points.iter().zip(moved.points) {
        assert_eq!(after.x - before.x, 10.0);
        assert_eq!(after.y, before.y);
    }
}
