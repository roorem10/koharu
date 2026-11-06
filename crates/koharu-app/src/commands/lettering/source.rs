use std::collections::BTreeSet;

use anyhow::Result;
use booharu_context::{LetteringCategory, ProjectContext, compare_source_style, measure_source_mask};
use koharu_pipeline::bubble_shape::{BubbleShape, text_bubble_shape};
use koharu_scene::{AssetRole, EntityId, Geometry, Snapshot};

use super::Project;

pub(crate) async fn refresh(
    project: &Project,
    layers: &[EntityId],
    persist: bool,
) -> Result<ProjectContext> {
    let snapshot = project.snapshot();
    let mut context = project.load_context()?;
    let original = context.clone();
    let mut pages = BTreeSet::new();
    for &layer in layers {
        snapshot.text_layer(layer)?;
        let mut page = layer;
        while let Some(parent) = snapshot.parent(page)? {
            page = parent;
        }
        pages.insert(page);
    }
    let mut results = Vec::new();
    for page in pages {
        let mask = if let Some(asset) = snapshot.asset(page, &AssetRole::new("source-text-mask")?)? {
            Some(image::load_from_memory(&snapshot.read_blob(asset.blob).await?)?.to_luma8())
        } else {
            None
        };
        let mut measured = Vec::new();
        let mut dialogue = Vec::new();
        for layer in snapshot.page(page)?.text_layers()? {
            let id = layer.id();
            let measurement = if let (Some(mask), Some(geometry)) =
                (mask.as_ref(), source_geometry(&snapshot, id)?)
            {
                crop(mask, &geometry).and_then(|crop| {
                    measure_source_mask(crop.width(), crop.height(), crop.as_raw())
                })
            } else {
                None
            };
            let info = text_bubble_shape(&snapshot, id)?;
            let is_dialogue = info.bubble.is_some()
                && info.shape != Some(BubbleShape::Rectangle)
                && context.category(id).is_none_or(|label| {
                    !matches!(
                        label.value,
                        LetteringCategory::Title
                            | LetteringCategory::Sfx
                            | LetteringCategory::PhoneText
                            | LetteringCategory::NarrationBox
                    )
                });
            if is_dialogue
                && let Some(measurement) = measurement
            {
                dialogue.push(measurement);
            }
            measured.push((id, measurement));
        }
        for (id, measurement) in measured {
            let style =
                measurement.and_then(|measurement| compare_source_style(measurement, &dialogue));
            context.set_source_style(id, style);
            results.push((id, style));
        }
    }
    if persist && context != original {
        project.update_context(|context| {
            for (id, style) in results {
                context.set_source_style(id, style);
            }
        })?;
    }
    Ok(context)
}

fn source_geometry(snapshot: &Snapshot, layer: EntityId) -> Result<Option<Geometry>> {
    let text = snapshot.text_layer(layer)?;
    // User frame edits must never change the measured source lettering.
    Ok(text
        .content()?
        .source_region()?
        .map(|region| region.geometry())
        .transpose()?)
}

fn crop(mask: &image::GrayImage, geometry: &Geometry) -> Option<image::GrayImage> {
    if geometry.points.len() < 3
        || geometry
            .points
            .iter()
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
    {
        return None;
    }
    let mut left = f64::INFINITY;
    let mut top = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    for point in &geometry.points {
        left = left.min(point.x);
        top = top.min(point.y);
        right = right.max(point.x);
        bottom = bottom.max(point.y);
    }
    let left = left.floor().max(0.0) as u32;
    let top = top.floor().max(0.0) as u32;
    let right = (right.ceil().max(0.0) as u32).min(mask.width());
    let bottom = (bottom.ceil().max(0.0) as u32).min(mask.height());
    if left >= right || top >= bottom {
        return None;
    }
    let width = right - left;
    let height = bottom - top;
    let mut pixels = vec![0; (width as usize).checked_mul(height as usize)?];
    let mut intersections = Vec::with_capacity(geometry.points.len());
    // Copy scanline spans inside the source polygon. A rotated box's bounding
    // rectangle can include a neighbouring box with completely different ink.
    for y in top..bottom {
        intersections.clear();
        let scan = f64::from(y) + 0.5;
        for (index, start) in geometry.points.iter().enumerate() {
            let end = geometry.points[(index + 1) % geometry.points.len()];
            if (start.y <= scan && end.y > scan) || (end.y <= scan && start.y > scan) {
                let crossing =
                    start.x + (scan - start.y) * (end.x - start.x) / (end.y - start.y);
                if !crossing.is_finite() {
                    return None;
                }
                intersections.push(crossing);
            }
        }
        intersections.sort_unstable_by(f64::total_cmp);
        for span in intersections.chunks_exact(2) {
            let start = ((span[0] - 0.5).ceil().max(0.0) as u32).clamp(left, right);
            let end = ((span[1] - 0.5).ceil().max(0.0) as u32).clamp(left, right);
            let source = y as usize * mask.width() as usize + start as usize;
            let target = (y - top) as usize * width as usize + (start - left) as usize;
            let length = (end - start) as usize;
            pixels[target..target + length]
                .copy_from_slice(&mask.as_raw()[source..source + length]);
        }
    }
    image::GrayImage::from_raw(width, height, pixels)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use booharu_context::SourceBold;
    use image::{DynamicImage, GrayImage, ImageFormat, Luma};
    use koharu_scene::{
        AssetInput, AssetMetadata, At, BubbleRegion, FlowsIn, Origin, PageDraft, Point,
        RecognizedFrom, TextLayout, TextLayoutKind, TextRegion,
    };

    use super::*;

    async fn fixture() -> (tempfile::TempDir, Project, Vec<EntityId>, Vec<Vec<EntityId>>) {
        let directory = tempfile::tempdir().unwrap();
        let mut project = Project::create(
            "source lettering".into(),
            directory.path().join("chapter.khrproj"),
        )
        .await
        .unwrap();
        let mut edit = project.snapshot().edit();
        let mut pages = Vec::new();
        let mut layers = Vec::new();
        for strokes in [[2, 2, 2, 4], [4, 4, 4, 4]] {
            let page = edit
                .add_page(PageDraft::new("page", 160.0, 160.0), At::End)
                .unwrap();
            let mut page_layers = Vec::new();
            let mut mask = GrayImage::new(160, 160);
            for (line, stroke) in strokes.into_iter().enumerate() {
                let y = line as u32 * 40;
                let bubble = edit
                    .add_analysis_region::<BubbleRegion>(
                        page,
                        At::End,
                        &Geometry {
                            origin: Origin::User,
                            points: (0..32)
                                .map(|index| {
                                    let angle = index as f64 * std::f64::consts::TAU / 32.0;
                                    Point {
                                        x: 80.0 + 76.0 * angle.cos(),
                                        y: f64::from(y) + 20.0 + 20.0 * angle.sin(),
                                    }
                                })
                                .collect(),
                        },
                        None,
                    )
                    .unwrap();
                let region = edit
                    .add_analysis_region::<TextRegion>(
                        page,
                        At::End,
                        &Geometry::rectangle(8.0, f64::from(y) + 4.0, 128.0, 32.0),
                        None,
                    )
                    .unwrap();
                let content = edit.add_text_content(page, At::End).unwrap();
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
                edit.set(layer, &Geometry::rectangle(145.0, 0.0, 10.0, 10.0))
                    .unwrap();
                for glyph in 0..5 {
                    for dy in 0..24 {
                        for dx in 0..16 {
                            if dx < stroke
                                || dx >= 16 - stroke
                                || (12..12 + stroke).contains(&dy)
                            {
                                mask.put_pixel(16 + glyph * 24 + dx, y + 8 + dy, Luma([255]));
                            }
                        }
                    }
                }
                page_layers.push(layer);
            }
            let mut png = Cursor::new(Vec::new());
            DynamicImage::ImageLuma8(mask)
                .write_to(&mut png, ImageFormat::Png)
                .unwrap();
            edit.set_asset(
                page,
                &AssetRole::new("source-text-mask").unwrap(),
                AssetInput::new(
                    png.into_inner(),
                    "image/png",
                    AssetMetadata {
                        width: Some(160),
                        height: Some(160),
                        attributes: Default::default(),
                    },
                ),
            )
            .unwrap();
            pages.push(page);
            layers.push(page_layers);
        }
        project.session.commit(edit.finish().unwrap()).await.unwrap();
        (directory, project, pages, layers)
    }

    #[tokio::test]
    async fn source_regions_and_page_medians_ignore_hand_frames_and_read_only_queries() {
        let (_directory, project, _pages, layers) = fixture().await;
        let requested = [layers[0][3], layers[1][3]];
        let revision = project.revision();
        let transient = refresh(&project, &requested, false).await.unwrap();
        assert_eq!(
            transient.source_style(layers[0][3]).unwrap().source_bold,
            SourceBold::Yes
        );
        assert_eq!(
            transient.source_style(layers[1][3]).unwrap().source_bold,
            SourceBold::No
        );
        assert!(!transient.source_style(layers[1][3]).unwrap().source_different);
        assert!(project.load_context().unwrap().lines.is_empty());
        assert_eq!(project.revision(), revision);
        let persisted = refresh(&project, &requested, true).await.unwrap();
        assert_eq!(project.load_context().unwrap(), persisted);
        assert_eq!(persisted, transient);
        assert_eq!(project.revision(), revision);
    }

    #[tokio::test]
    async fn missing_source_mask_clears_old_evidence_only_when_persisting() {
        let (_directory, mut project, pages, layers) = fixture().await;
        refresh(&project, &[layers[0][3]], true).await.unwrap();
        let snapshot = project.snapshot();
        let patch = snapshot
            .patch(|edit| edit.remove_asset(pages[0], &AssetRole::new("source-text-mask")?))
            .unwrap();
        project.session.commit(patch).await.unwrap();
        let transient = refresh(&project, &[layers[0][3]], false).await.unwrap();
        assert!(transient.source_style(layers[0][3]).is_none());
        assert!(
            project
                .load_context()
                .unwrap()
                .source_style(layers[0][3])
                .is_some()
        );
        refresh(&project, &[layers[0][3]], true).await.unwrap();
        assert!(project.load_context().unwrap().lines.is_empty());
    }

    #[test]
    fn polygon_crop_excludes_neighbouring_ink_in_rotated_bounds() {
        let mask = GrayImage::from_pixel(8, 8, Luma([255]));
        let geometry = Geometry {
            origin: Origin::User,
            points: vec![
                Point { x: 4.0, y: 0.0 },
                Point { x: 8.0, y: 4.0 },
                Point { x: 4.0, y: 8.0 },
                Point { x: 0.0, y: 4.0 },
            ],
        };
        let cropped = crop(&mask, &geometry).unwrap();
        assert_eq!(cropped.get_pixel(0, 0)[0], 0);
        assert_eq!(cropped.get_pixel(7, 7)[0], 0);
        assert_eq!(cropped.get_pixel(4, 4)[0], 255);
        assert_eq!(
            crop(&mask, &Geometry::rectangle(20.0, 20.0, 5.0, 5.0)),
            None
        );
    }
}
