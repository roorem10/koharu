use koharu_scene::{At, Authored, BubbleRegion, PageDraft, Point, Session, SourceText, TextRegion};

use super::*;

#[tokio::test]
async fn joined_balloon_drag_and_wide_resize_preserve_placement_and_visible_pixels() {
    let mut session = Session::memory().await.unwrap();
    let mut ids = None;
    let create = session
        .snapshot()
        .patch(|edit| {
            let page = edit.add_page(PageDraft::new("page", 600.0, 240.0), At::End)?;
            let bubble = edit.add_analysis_region::<BubbleRegion>(
                page,
                At::End,
                &Geometry {
                    origin: Origin::User,
                    points: [
                        (20.0, 40.0),
                        (100.0, 40.0),
                        (120.0, 90.0),
                        (140.0, 40.0),
                        (220.0, 40.0),
                        (220.0, 160.0),
                        (140.0, 160.0),
                        (120.0, 110.0),
                        (100.0, 160.0),
                        (20.0, 160.0),
                    ]
                    .into_iter()
                    .map(|(x, y)| Point { x, y })
                    .collect(),
                },
                None,
            )?;
            let mut layers = Vec::new();
            for (x, text) in [(50.0, "First flow"), (150.0, "Second flow")] {
                let region = edit.add_analysis_region::<TextRegion>(
                    page,
                    At::End,
                    &Geometry::rectangle(x, 80.0, 20.0, 40.0),
                    None,
                )?;
                let content = edit.add_text_content(page, At::End)?;
                edit.set(
                    content,
                    &SourceText {
                        text: Authored::user(text.to_owned()),
                        language: None,
                    },
                )?;
                edit.set(
                    content,
                    &Translation {
                        text: Authored::user(text.to_owned()),
                        language: None,
                    },
                )?;
                edit.relate::<RecognizedFrom>(content, region)?;
                let layer = edit.add_text_layer(
                    page,
                    At::End,
                    content,
                    &SceneTextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: None,
                        warp: None,
                    },
                )?;
                edit.relate::<FlowsIn>(layer, bubble)?;
                layers.push(layer);
            }
            ids = Some((page, layers));
            Ok(())
        })
        .unwrap();
    let base = session.commit(create).await.unwrap().snapshot;
    let (page, layers) = ids.unwrap();
    let font = crate::fonts::FontSystem::new().first_font().unwrap();
    let renderer = Renderer::with_typesetting(TypesettingConfig {
        font_families: vec![font.family_name().to_owned()],
    });
    let original = renderer.render(&base, page).await.unwrap();
    let first = original.layer(layers[0]).unwrap();
    let sibling = original.layer(layers[1]).unwrap();
    assert!(first.geometry().points.iter().all(|point| point.x <= 120.0));
    assert!(sibling.geometry().points.iter().all(|point| point.x >= 120.0));
    let LayerKind::Text(first_text) = first.kind() else {
        panic!("expected text");
    };

    let rasterizer = Rasterizer::new().unwrap();
    let raster = |frame: &Frame| {
        rasterizer
            .rasterize(&frame.raster_frame().unwrap(), RasterOptions::default())
            .unwrap()
            .image
    };
    let assert_same = |left: &Frame, right: &Frame| {
        assert_eq!(raster(left), raster(right));
        assert_eq!(left.diagnostics(), right.diagnostics());
        for &entity in &layers {
            assert_eq!(
                left.layer(entity).unwrap().geometry(),
                right.layer(entity).unwrap().geometry()
            );
            assert_eq!(
                left.layer(entity).unwrap().kind(),
                right.layer(entity).unwrap().kind()
            );
            let left_crop = left.cropped(entity).unwrap().unwrap();
            let right_crop = right.cropped(entity).unwrap().unwrap();
            assert_eq!(left_crop.origin(), right_crop.origin());
            let pixels = raster(&left_crop);
            assert_eq!(pixels, raster(&right_crop));
            assert!(pixels.pixels().any(|pixel| pixel[3] != 0));
            let editable = left.cropped_unwarped_text(entity).unwrap().unwrap();
            assert_eq!(editable.origin(), left_crop.origin());
            assert_eq!(raster(&editable), pixels);
        }
    };

    let mut previous = original.clone();
    for scale_x in [1.0, 4.0] {
        let snapshot = session.snapshot();
        let mut geometry = first.geometry().clone();
        for point in &mut geometry.points {
            point.x = 20.0 + (point.x - 20.0) * scale_x + 70.0;
        }
        let mut layout = snapshot.text_layer(layers[0]).unwrap().layout().unwrap();
        layout.angle_degrees = Some(first_text.angle_degrees);
        // A completed native transform authors the displayed polygon and its axes
        // together. The source anchor must still reserve the sibling's old cell.
        let patch = snapshot
            .patch(|edit| {
                edit.set(layers[0], &geometry)?;
                edit.set(layers[0], &layout)
            })
            .unwrap();
        let changed = session.commit(patch).await.unwrap();
        let incremental = renderer
            .update(&previous, &changed.snapshot, &changed.changes)
            .await
            .unwrap();
        let full = renderer.render(&changed.snapshot, page).await.unwrap();
        assert_same(&incremental, &full);
        assert_eq!(incremental.layer(layers[0]).unwrap().geometry(), &geometry);
        assert_eq!(
            incremental.layer(layers[1]).unwrap().geometry(),
            sibling.geometry()
        );
        assert_eq!(incremental.layer(layers[1]).unwrap().kind(), sibling.kind());
        if scale_x == 1.0 {
            let LayerKind::Text(moved_text) = incremental.layer(layers[0]).unwrap().kind() else {
                panic!("expected text");
            };
            assert!(
                (moved_text.rendered_bounds.x - first_text.rendered_bounds.x - 70.0).abs()
                    < 1e-4
            );
            assert!(
                (moved_text.layout_bounds.x - first_text.layout_bounds.x - 70.0).abs() < 1e-4
            );
            let mut translated = first_text.clone();
            translated.rendered_bounds.x = moved_text.rendered_bounds.x;
            translated.layout_bounds.x = moved_text.layout_bounds.x;
            assert_eq!(moved_text, &translated);
        }

        let undone = session.undo(changed.revision).await.unwrap();
        let restored = renderer
            .update(&incremental, &undone.snapshot, &undone.changes)
            .await
            .unwrap();
        assert_same(&restored, &previous);
        let redone = session.undo(undone.revision).await.unwrap();
        let reapplied = renderer
            .update(&restored, &redone.snapshot, &redone.changes)
            .await
            .unwrap();
        assert_same(&reapplied, &full);
        previous = reapplied;
    }
}
