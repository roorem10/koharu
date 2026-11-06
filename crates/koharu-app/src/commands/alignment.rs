use std::collections::{BTreeMap, HashSet};

use anyhow::{Context as _, Result, ensure};
use koharu_desktop::Desktop;
use koharu_rasterizer::PreparedElementFrame;
use koharu_renderer::Renderer;
use koharu_scene::{Commit, EntityId, Geometry, Origin, Patch, Point, Revision, Snapshot};
use tauri::State;

use super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    project::{CurrentProject, Project},
};

#[tauri::command]
#[specta::specta]
pub(crate) async fn center_in_bubble(
    expected_revision: Revision,
    layers: Vec<EntityId>,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<Option<Revision>, Error> {
    let renderer = desktop.renderer();
    let (commit, page) = {
        let mut project = project.project.lock().await;
        let project = project.as_mut().context("no project is open")?;
        let commit = project
            .center_in_bubble(expected_revision, layers, &renderer)
            .await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, project.active_page())
    };
    let Some(commit) = commit else {
        return Ok(None);
    };
    desktop.synchronize(&commit.snapshot, page, &commit).await?;
    canvas_channel.channel.publish(desktop.canvas_state());
    Ok(Some(commit.revision))
}

impl Project {
    pub(crate) async fn center_in_bubble(
        &mut self,
        expected_revision: Revision,
        layers: Vec<EntityId>,
        renderer: &Renderer,
    ) -> Result<Option<Commit>> {
        let snapshot = self.snapshot();
        ensure!(
            snapshot.revision() == expected_revision,
            "centering expected revision {expected_revision}, but the project is at {}",
            snapshot.revision()
        );
        let mut seen = HashSet::new();
        let mut pages = BTreeMap::<EntityId, Vec<EntityId>>::new();
        for layer in layers {
            if !seen.insert(layer) {
                continue;
            }
            let text = snapshot.text_layer(layer)?;
            if text.balloon_target()?.is_none() {
                continue;
            }
            let mut page = layer;
            while let Some(parent) = snapshot.parent(page)? {
                page = parent;
            }
            snapshot.page(page)?;
            pages.entry(page).or_default().push(layer);
        }
        let mut prepared = Vec::new();
        for (page, layers) in pages {
            let frame = renderer.render(&snapshot, page).await?;
            for layer in layers {
                let Some(rendered) = frame.layer(layer) else {
                    continue;
                };
                let Some(control) = rendered.element_frame() else {
                    continue;
                };
                prepared.push((layer, rendered.geometry().clone(), control));
            }
        }
        let Some(patch) = center_patch(&snapshot, prepared)? else {
            return Ok(None);
        };
        Ok(Some(self.commit(patch).await?))
    }
}

fn center_patch(
    snapshot: &Snapshot,
    prepared: impl IntoIterator<Item = (EntityId, Geometry, PreparedElementFrame)>,
) -> Result<Option<Patch>> {
    let mut updates = Vec::new();
    for (layer, mut geometry, frame) in prepared {
        ensure!(frame.is_valid(), "centering requires a valid rendered frame");
        let text = snapshot.text_layer(layer)?;
        let Some(bubble) = text.balloon_target()? else {
            continue;
        };
        let to = bounds_center(&bubble.geometry()?)?;
        // Match canvas dragging: the control frame follows visible text, while the
        // persisted contour may be larger or be one cell of a joined balloon.
        let delta = Point {
            x: to.x - (f64::from(frame.x) + f64::from(frame.width) * 0.5),
            y: to.y - (f64::from(frame.y) + f64::from(frame.height) * 0.5),
        };
        // Prepared frames use f32; repeated centering must not accumulate subpixel edits.
        let tolerance = (to.x.abs().max(to.y.abs()) * f64::from(f32::EPSILON) * 2.0).max(1e-4);
        if delta.x.abs() <= tolerance && delta.y.abs() <= tolerance {
            continue;
        }
        for point in &mut geometry.points {
            point.x += delta.x;
            point.y += delta.y;
        }
        geometry.origin = Origin::User;
        let mut layout = text.layout()?;
        layout.origin = Origin::User;
        layout.angle_degrees = Some(frame.angle_degrees);
        updates.push((layer, text.content()?.id(), geometry, layout));
    }
    if updates.is_empty() {
        return Ok(None);
    }
    Ok(Some(snapshot.patch(|edit| {
        for (layer, content, geometry, layout) in updates {
            // Layout observes the page, including sibling flows sharing this bubble.
            edit.observe_subtree(layer)?;
            edit.promote_entity_to_user(layer)?;
            edit.promote_entity_to_user(content)?;
            edit.set(layer, &geometry)?;
            edit.set(layer, &layout)?;
        }
        Ok(())
    })?))
}

fn bounds_center(geometry: &Geometry) -> Result<Point> {
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for point in &geometry.points {
        ensure!(
            point.x.is_finite() && point.y.is_finite(),
            "invalid frame geometry"
        );
        bounds[0] = bounds[0].min(point.x);
        bounds[1] = bounds[1].min(point.y);
        bounds[2] = bounds[2].max(point.x);
        bounds[3] = bounds[3].max(point.y);
    }
    ensure!(
        bounds.iter().all(|value| value.is_finite())
            && bounds[2] > bounds[0]
            && bounds[3] > bounds[1],
        "centering requires nonempty frame geometry"
    );
    Ok(Point {
        x: bounds[0] + (bounds[2] - bounds[0]) / 2.0,
        y: bounds[1] + (bounds[3] - bounds[1]) / 2.0,
    })
}

#[cfg(test)]
mod tests {
    use koharu_scene::{At, BubbleRegion, FlowsIn, PageDraft, Session, TextLayout, TextLayoutKind};

    use super::*;

    async fn project() -> (Project, EntityId, Vec<EntityId>, Vec<Geometry>) {
        let mut session = Session::memory().await.unwrap();
        let mut edit = session.snapshot().edit();
        let page = edit
            .add_page(PageDraft::new("page", 400.0, 300.0), At::End)
            .unwrap();
        let bubble = edit
            .add_analysis_region::<BubbleRegion>(
                page,
                At::End,
                &Geometry::rectangle(100.0, 80.0, 200.0, 120.0),
                None,
            )
            .unwrap();
        let mut layers = Vec::new();
        let geometries = vec![
            Geometry::rectangle(10.0, 20.0, 40.0, 60.0),
            Geometry {
                origin: Origin::User,
                points: vec![
                    Point { x: 10.0, y: 30.0 },
                    Point { x: 50.0, y: 10.0 },
                    Point { x: 70.0, y: 50.0 },
                    Point { x: 30.0, y: 70.0 },
                ],
            },
        ];
        for geometry in &geometries {
            let content = edit.add_text_content(page, At::End).unwrap();
            let layer = edit
                .add_text_layer(
                    page,
                    At::End,
                    content,
                    &TextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: Some(27.0),
                        warp: None,
                    },
                )
                .unwrap();
            edit.set(layer, geometry).unwrap();
            edit.relate::<FlowsIn>(layer, bubble).unwrap();
            layers.push(layer);
        }
        session.commit(edit.finish().unwrap()).await.unwrap();
        (
            Project::new(session, "test".into()),
            bubble,
            layers,
            geometries,
        )
    }

    #[tokio::test]
    async fn centers_selection_in_one_undo_preserving_shape_rotation_and_bubble() {
        let (mut project, bubble, layers, originals) = project().await;
        let control = PreparedElementFrame {
            x: 20.0,
            y: 30.0,
            width: 10.0,
            height: 12.0,
            angle_degrees: 27.0,
        };
        let patch = center_patch(
            &project.snapshot(),
            layers
                .iter()
                .zip(&originals)
                .map(|(&layer, geometry)| (layer, geometry.clone(), control)),
        )
        .unwrap()
        .unwrap();
        let commit = project.commit(patch).await.unwrap();
        project.record_commit(&commit);
        assert_eq!(project.undo.len(), 1);
        for (&layer, original) in layers.iter().zip(&originals) {
            let snapshot = project.snapshot();
            let text = snapshot.text_layer(layer).unwrap();
            let frame = text.frame().unwrap().unwrap();
            let delta = Point {
                x: frame.points[0].x - original.points[0].x,
                y: frame.points[0].y - original.points[0].y,
            };
            assert_eq!(delta, Point { x: 175.0, y: 104.0 });
            for (before, after) in original.points.iter().zip(&frame.points) {
                assert_eq!(after.x - before.x, delta.x);
                assert_eq!(after.y - before.y, delta.y);
            }
            assert_eq!(text.layout().unwrap().angle_degrees, Some(27.0));
            assert_eq!(text.balloon_target().unwrap().unwrap().id(), bubble);
        }
        assert_eq!(
            project
                .snapshot()
                .analysis_region(bubble)
                .unwrap()
                .geometry()
                .unwrap(),
            Geometry::rectangle(100.0, 80.0, 200.0, 120.0)
        );
        let snapshot = project.snapshot();
        assert!(
            center_patch(
                &snapshot,
                layers.iter().map(|&layer| (
                    layer,
                    snapshot.text_layer(layer).unwrap().frame().unwrap().unwrap(),
                    PreparedElementFrame {
                        x: 195.0,
                        y: 134.0,
                        ..control
                    },
                )),
            )
            .unwrap()
            .is_none()
        );
        project.undo().await.unwrap();
        for (&layer, original) in layers.iter().zip(&originals) {
            assert_eq!(
                project
                    .snapshot()
                    .text_layer(layer)
                    .unwrap()
                    .frame()
                    .unwrap()
                    .as_ref(),
                Some(original)
            );
        }
        assert!(project.undo.is_empty());
    }

    #[tokio::test]
    async fn stale_centering_cannot_overwrite_a_newer_edit() {
        let (mut project, _, layers, originals) = project().await;
        let snapshot = project.snapshot();
        let prepared = vec![(
            layers[0],
            originals[0].clone(),
            PreparedElementFrame {
                x: 10.0,
                y: 20.0,
                width: 40.0,
                height: 60.0,
                angle_degrees: 27.0,
            },
        )];
        let stale = center_patch(&snapshot, prepared.clone()).unwrap().unwrap();
        let current = center_patch(&snapshot, prepared).unwrap().unwrap();
        project.commit(current).await.unwrap();
        let current = project.snapshot().revision();
        assert!(project.commit(stale).await.is_err());
        assert_eq!(project.snapshot().revision(), current);
    }

    #[tokio::test]
    async fn automatic_text_centers_its_rendered_cell_instead_of_the_whole_bubble() {
        let (mut project, _, layers, cells) = project().await;
        let layer = layers[0];
        let patch = project
            .snapshot()
            .patch(|edit| edit.remove::<Geometry>(layer))
            .unwrap();
        project.commit(patch).await.unwrap();
        let patch = center_patch(
            &project.snapshot(),
            [(
                layer,
                cells[0].clone(),
                PreparedElementFrame {
                    x: 20.0,
                    y: 30.0,
                    width: 10.0,
                    height: 12.0,
                    angle_degrees: 27.0,
                },
            )],
        )
        .unwrap()
        .unwrap();
        let commit = project.commit(patch).await.unwrap();
        project.record_commit(&commit);
        let geometry = project
            .snapshot()
            .component::<Geometry>(layer)
            .unwrap()
            .unwrap();
        assert_eq!(geometry, Geometry::rectangle(185.0, 124.0, 40.0, 60.0));
        project.undo().await.unwrap();
        assert!(
            project
                .snapshot()
                .component::<Geometry>(layer)
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn prepared_frame_rounding_at_large_coordinates_does_not_create_an_undo_step() {
        let (mut project, bubble, layers, _) = project().await;
        let page = project.snapshot().pages().next().unwrap().id();
        let patch = project
            .snapshot()
            .patch(|edit| {
                edit.set_page(page, PageDraft::new("large page", 20_000.0, 20_000.0))?;
                edit.set(bubble, &Geometry::rectangle(9_980.0, 9_980.0, 40.1234, 40.1234))
            })
            .unwrap();
        project.commit(patch).await.unwrap();
        let revision = project.snapshot().revision();
        let prepared = PreparedElementFrame {
            x: 9_990.0617_f64 as f32,
            y: 9_990.0617_f64 as f32,
            width: 20.0,
            height: 20.0,
            angle_degrees: 27.0,
        };
        assert!(
            center_patch(
                &project.snapshot(),
                [(
                    layers[0],
                    Geometry::rectangle(9_980.0, 9_980.0, 40.1234, 40.1234),
                    prepared,
                )],
            )
            .unwrap()
            .is_none()
        );
        assert_eq!(project.snapshot().revision(), revision);
        assert!(project.undo.is_empty());
    }
}
