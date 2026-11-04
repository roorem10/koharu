use std::io::Cursor;

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use koharu_scene::{
    AssetInput, AssetMetadata, AssetRole, At, Origin, PageDraft, RasterLayer, RasterLayerKind,
    Session,
};

use super::*;
use crate::commands::project::{Project, RasterStrokeMode};

#[test]
fn hardness_controls_edge_coverage_and_keeps_the_centre_solid() {
    let point = [Point { x: 8.5, y: 8.5 }];
    let mut soft = GrayImage::new(18, 18);
    stroke::add_stroke(&mut soft, &point, 10.0, 0.0);
    let mut hard = GrayImage::new(18, 18);
    stroke::add_stroke(&mut hard, &point, 10.0, 100.0);
    assert_eq!(soft.get_pixel(8, 8)[0], 255);
    assert!((1..255).contains(&soft.get_pixel(12, 8)[0]));
    assert_eq!(hard.get_pixel(12, 8)[0], 255);
    assert_eq!(hard.get_pixel(14, 8)[0], 0);
    assert!(stroke::validate(&point, 10.0, f32::NAN).is_err());
    let mut integer_centre = GrayImage::new(18, 18);
    stroke::add_stroke(&mut integer_centre, &[Point { x: 8., y: 8. }], 10., 0.);
    assert_eq!(integer_centre.get_pixel(8, 8)[0], 255);
}

#[test]
fn two_rectangles_share_one_mask_and_cover_both_bounds() {
    let mask = stroke::areas_mask(
        30,
        20,
        &[],
        &[
            CleanupRect {
                x: 2.,
                y: 3.,
                width: 4.,
                height: 5.,
            },
            CleanupRect {
                x: 20.,
                y: 12.,
                width: 6.,
                height: 4.,
            },
        ],
    )
    .unwrap();
    let (png, bounds) = encode(&mask).unwrap();
    let decoded = image::load_from_memory(&png).unwrap().to_luma8();
    assert_eq!(decoded.get_pixel(3, 4)[0], 255);
    assert_eq!(decoded.get_pixel(22, 13)[0], 255);
    assert_eq!(decoded.get_pixel(12, 10)[0], 0);
    assert_eq!(
        (bounds.x, bounds.y, bounds.width, bounds.height),
        (2., 3., 24., 13.)
    );
}

#[test]
fn masks_clip_to_page_and_reject_invalid_areas() {
    let mask = stroke::areas_mask(
        4,
        4,
        &[],
        &[CleanupRect {
            x: -5.,
            y: -3.,
            width: 8.,
            height: 5.,
        }],
    )
    .unwrap();
    assert_eq!(mask.pixels().filter(|p| p[0] != 0).count(), 6);
    assert!(
        stroke::areas_mask(
            4,
            4,
            &[],
            &[CleanupRect {
                x: f64::NAN,
                y: 0.,
                width: 1.,
                height: 1.
            },]
        )
        .is_err()
    );
}

#[tokio::test]
async fn default_target_uses_cleanup_and_erase_reveals_source_with_one_undo_per_stroke() {
    let config = koharu_config::load::<CleanupSettings>("cleanup").unwrap();
    let previous = config.read().unwrap().clone();
    config.write().unwrap().paint_target = koharu_pipeline::PaintTarget::Cleanup;
    let mut session = Session::memory().await.unwrap();
    let mut page = None;
    let source = RgbaImage::from_pixel(16, 16, Rgba([20, 40, 80, 255]));
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(source.clone())
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    let patch = session
        .snapshot()
        .patch(|edit| {
            let id = edit.add_page(PageDraft::new("page", 16., 16.), At::End)?;
            edit.set_asset(
                id,
                &AssetRole::new("source")?,
                AssetInput::new(
                    bytes.into_inner(),
                    "image/png",
                    AssetMetadata {
                        width: Some(16),
                        height: Some(16),
                        attributes: Default::default(),
                    },
                ),
            )?;
            page = Some(id);
            Ok(())
        })
        .unwrap();
    session.commit(patch).await.unwrap();
    let page = page.unwrap();
    let mut project = Project::new(session, "cleanup".into());
    let points = || vec![koharu_scene::Point { x: 8.5, y: 8.5 }];
    let (paint, layer) = project
        .apply_raster_stroke(
            page,
            None,
            RasterStrokeMode::Paint,
            [255; 4],
            6.,
            100.,
            points(),
        )
        .await
        .unwrap();
    project.record_commit(&paint);
    let raster = paint
        .snapshot
        .component::<RasterLayer>(layer)
        .unwrap()
        .unwrap();
    assert_eq!(raster.kind, RasterLayerKind::Cleanup);
    assert_eq!(raster.origin, Origin::User);
    let (erase, same_layer) = project
        .apply_raster_stroke(
            page,
            None,
            RasterStrokeMode::Erase,
            [0; 4],
            6.,
            100.,
            points(),
        )
        .await
        .unwrap();
    project.record_commit(&erase);
    assert_eq!(layer, same_layer);
    let read = |snapshot: koharu_scene::Snapshot| async move {
        let asset = snapshot
            .asset(layer, &AssetRole::new("source").unwrap())
            .unwrap()
            .unwrap();
        image::load_from_memory(&snapshot.read_blob(asset.blob).await.unwrap())
            .unwrap()
            .to_rgba8()
    };
    let mut composite = source.clone();
    image::imageops::overlay(&mut composite, &read(erase.snapshot.clone()).await, 0, 0);
    assert_eq!(composite.get_pixel(8, 8), source.get_pixel(8, 8));
    assert_eq!(composite.get_pixel(8, 8)[3], 255);
    let undone = project.undo().await.unwrap();
    assert_eq!(read(undone.snapshot).await.get_pixel(8, 8).0, [255; 4]);
    let undone = project.undo().await.unwrap();
    assert!(
        !undone
            .snapshot
            .children(page)
            .unwrap()
            .any(|id| id == layer)
    );
    let mut selected = None;
    let patch = project
        .snapshot()
        .patch(|edit| {
            let id = edit.add_entity(page, At::End)?;
            edit.set(
                id,
                &RasterLayer {
                    origin: Origin::User,
                    name: "Paint".into(),
                    kind: RasterLayerKind::Paint,
                },
            )?;
            selected = Some(id);
            Ok(())
        })
        .unwrap();
    project.commit(patch).await.unwrap();
    let (paint, chosen) = project
        .apply_raster_stroke(
            page,
            selected,
            RasterStrokeMode::Paint,
            [255, 255, 255, 128],
            6.,
            100.,
            vec![points()[0], points()[0], points()[0]],
        )
        .await
        .unwrap();
    assert_eq!(Some(chosen), selected);
    let asset = paint
        .snapshot
        .asset(chosen, &AssetRole::new("source").unwrap())
        .unwrap()
        .unwrap();
    let painted = image::load_from_memory(&paint.snapshot.read_blob(asset.blob).await.unwrap())
        .unwrap()
        .to_rgba8();
    assert_eq!(
        painted.get_pixel(8, 8)[3],
        128,
        "repeated pointer samples must not compound opacity"
    );
    config.write().unwrap().paint_target = koharu_pipeline::PaintTarget::PaintLayer;
    let (paint, chosen) = project
        .apply_raster_stroke(
            page,
            None,
            RasterStrokeMode::Paint,
            [255; 4],
            6.,
            100.,
            points(),
        )
        .await
        .unwrap();
    assert_eq!(
        paint
            .snapshot
            .component::<RasterLayer>(chosen)
            .unwrap()
            .unwrap()
            .kind,
        RasterLayerKind::Paint
    );
    assert!(
        project
            .apply_raster_stroke(
                page,
                None,
                RasterStrokeMode::Erase,
                [0; 4],
                6.,
                100.,
                points()
            )
            .await
            .is_err()
    );
    *config.write().unwrap() = previous;
}
