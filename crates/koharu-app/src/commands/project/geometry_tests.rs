use koharu_scene::{
    At, Authored, BubbleRegion, EntityOrigin, FlowsIn, Generation, Geometry, Origin, PageDraft,
    Point, ProducerId, RecognizedFrom, Session, SourceText, TextLayout, TextLayoutKind, TextRegion,
    Translation,
};

use super::Project;
use crate::commands::{canvas, editing::GeometryUpdate};

#[tokio::test]
async fn agent_and_drag_geometry_preserve_shared_bubble_provenance_in_one_undo() {
    for agent_edit in [false, true] {
        let mut session = Session::memory().await.unwrap();
        let mut edit = session.snapshot().edit();
        let page = edit
            .add_page(PageDraft::new("joined bubble", 300.0, 200.0), At::End)
            .unwrap();
        session.commit(edit.finish().unwrap()).await.unwrap();
        let generation = Generation::new(ProducerId::new("test.detection").unwrap());
        let mut edit = session.snapshot().edit_as(generation.clone());
        let bubble = edit
            .add_analysis_region::<BubbleRegion>(
                page,
                At::End,
                &Geometry {
                    origin: Origin::User,
                    points: [
                        (10.0, 10.0),
                        (100.0, 10.0),
                        (100.0, 50.0),
                        (130.0, 50.0),
                        (130.0, 10.0),
                        (220.0, 10.0),
                        (220.0, 110.0),
                        (130.0, 110.0),
                        (130.0, 70.0),
                        (100.0, 70.0),
                        (100.0, 110.0),
                        (10.0, 110.0),
                    ]
                    .into_iter()
                    .map(|(x, y)| Point { x, y })
                    .collect(),
                },
                None,
            )
            .unwrap();
        let mut layers = Vec::new();
        for x in [30.0, 150.0] {
            let region = edit
                .add_analysis_region::<TextRegion>(
                    page,
                    At::End,
                    &Geometry::rectangle(x, 30.0, 40.0, 60.0),
                    None,
                )
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
                    text: Authored::generated("translation".into(), generation.clone()),
                    language: None,
                },
            )
            .unwrap();
            edit.relate::<RecognizedFrom>(content, region).unwrap();
            let layer = edit
                .add_text_layer(
                    page,
                    At::End,
                    content,
                    &TextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: None,
                        warp: None,
                    },
                )
                .unwrap();
            edit.relate::<FlowsIn>(layer, bubble).unwrap();
            layers.push((layer, content, region));
        }
        session.commit(edit.finish().unwrap()).await.unwrap();
        let mut project = Project::new(session, "geometry".into());
        let before = project.snapshot();
        let selected = layers[0].0;
        let dropped = Geometry::rectangle(85.0, 30.0, 110.0, 60.0);
        let commit = if agent_edit {
            project
                .set_geometry(vec![GeometryUpdate {
                    layer: selected,
                    points: Some(
                        dropped
                            .points
                            .iter()
                            .map(|point| canvas::Point {
                                x: point.x,
                                y: point.y,
                            })
                            .collect(),
                    ),
                }])
                .await
                .unwrap()
        } else {
            project
                .set_geometries([(selected, dropped.clone(), 0.0)])
                .await
                .unwrap()
        };
        project.record_commit(&commit);
        assert_eq!(project.undo.len(), 1);
        assert_eq!(
            commit.snapshot.component::<Geometry>(selected).unwrap(),
            Some(dropped.clone())
        );
        let undone = project.undo().await.unwrap();
        assert!(project.undo.is_empty());
        assert_eq!(
            undone.snapshot.component::<Geometry>(selected).unwrap(),
            None
        );
        assert_eq!(
            undone.snapshot.component::<TextLayout>(selected).unwrap(),
            before.component::<TextLayout>(selected).unwrap()
        );
        assert_eq!(
            undone.snapshot.component::<EntityOrigin>(selected).unwrap(),
            before.component::<EntityOrigin>(selected).unwrap()
        );
        let redone = project.redo().await.unwrap();
        assert_eq!(project.undo.len(), 1);
        assert_eq!(
            redone.snapshot.component::<Geometry>(selected).unwrap(),
            Some(dropped)
        );
        assert_eq!(
            redone.snapshot.component::<TextLayout>(selected).unwrap(),
            commit.snapshot.component::<TextLayout>(selected).unwrap()
        );

        // Authored geometry must keep the source anchor's reserved share, so the
        // sibling's automatic placement and OCR provenance survive undo/redo.
        for snapshot in [&commit.snapshot, &undone.snapshot, &redone.snapshot] {
            assert_eq!(
                snapshot.component::<Geometry>(bubble).unwrap(),
                before.component::<Geometry>(bubble).unwrap()
            );
            assert!(snapshot.component::<Geometry>(layers[1].0).unwrap().is_none());
            assert_eq!(
                snapshot.component::<TextLayout>(layers[1].0).unwrap(),
                before.component::<TextLayout>(layers[1].0).unwrap()
            );
            for &(layer, content, region) in &layers {
                assert_eq!(
                    snapshot
                        .relation_from::<FlowsIn>(layer)
                        .unwrap()
                        .unwrap()
                        .value()
                        .target,
                    bubble
                );
                assert_eq!(
                    snapshot
                        .relation_from::<RecognizedFrom>(content)
                        .unwrap()
                        .unwrap()
                        .value()
                        .target,
                    region
                );
                assert_eq!(
                    snapshot.component::<Geometry>(region).unwrap(),
                    before.component::<Geometry>(region).unwrap()
                );
                assert_eq!(
                    snapshot.component::<SourceText>(content).unwrap(),
                    before.component::<SourceText>(content).unwrap()
                );
                assert_eq!(
                    snapshot.component::<Translation>(content).unwrap(),
                    before.component::<Translation>(content).unwrap()
                );
            }
        }
    }
}
