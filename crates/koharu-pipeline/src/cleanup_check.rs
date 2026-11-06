//! Cleanup verification uses the ordinary inpainting job, with a single
//! guarded page patch containing the second attempt and its QC results.

use std::{collections::BTreeMap, io::Cursor, sync::Arc};

use anyhow::{Context as _, Result, ensure};
use image::{DynamicImage, GenericImageView as _, GrayImage, ImageFormat, Luma, RgbaImage};
use imageproc::{distance_transform::Norm, morphology::dilate};
use koharu_scene::{
    Asset, AssetInput, AssetRole, BlobId, EntityId, Origin, Patch, RasterLayer, RasterLayerKind,
    Snapshot,
};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    Bounds, CleanupSettings, InpaintingMask, ProgressSink, StopToken, stage_runner::StageRunner,
    stages::StageInput,
};

const BASELINE: &str = "dev.booharu.cleanup-generated";
const REPORT: &str = "dev.booharu.cleanup-check";

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct CleanupCheckResult {
    pub page: EntityId,
    pub layer: EntityId,
    pub findings: Vec<Bounds>,
    pub retried: bool,
    pub protected: bool,
}

#[derive(Deserialize, Serialize)]
struct StoredResult {
    result: CleanupCheckResult,
    original: BlobId,
    cleanup: BlobId,
    text_mask: Option<BlobId>,
    coo_mask: Option<BlobId>,
}

/// Reports are attached to the same immutable pixels they describe. Painting,
/// replacing the original, or detecting new text invalidates them immediately.
pub fn results(snapshot: &Snapshot) -> Result<Vec<CleanupCheckResult>> {
    let mut results = Vec::new();
    let source = AssetRole::new("source")?;
    for page in snapshot.pages() {
        for layer in snapshot.children(page.id())? {
            if !snapshot
                .component::<RasterLayer>(layer)?
                .is_some_and(|value| value.kind == RasterLayerKind::Cleanup)
            {
                continue;
            }
            let Some(asset) = snapshot.asset(layer, &source)? else {
                continue;
            };
            if let Some(result) = stored_result(snapshot, page.id(), layer, &asset)? {
                results.push(result);
            }
        }
    }
    Ok(results)
}

fn stored_result(
    snapshot: &Snapshot,
    page: EntityId,
    layer: EntityId,
    asset: &Asset,
) -> Result<Option<CleanupCheckResult>> {
    let Some(value) = asset.metadata.attributes.get(REPORT) else {
        return Ok(None);
    };
    let stored: StoredResult =
        serde_json::from_str(value).context("cleanup check metadata is invalid")?;
    let valid = stored.result.page == page
        && stored.result.layer == layer
        && stored.cleanup == asset.blob
        && Some(stored.original) == blob(snapshot, page, "source")?
        && stored.text_mask == blob(snapshot, page, "text-mask")?
        && stored.coo_mask == blob(snapshot, page, "coo-mask")?;
    Ok(valid.then_some(stored.result))
}

fn blob(snapshot: &Snapshot, entity: EntityId, role: &str) -> Result<Option<BlobId>> {
    Ok(snapshot
        .asset(entity, &AssetRole::new(role)?)?
        .map(|asset| asset.blob))
}

fn protected(asset: &Asset) -> bool {
    asset.metadata.attributes.get(BASELINE).map_or_else(
        || asset.origin == Origin::User,
        |baseline| *baseline != asset.blob.to_string(),
    )
}

/// Normal cleanup also records a baseline. A later explicit AI removal must
/// not make earlier hand-painted pixels eligible for automatic repainting.
pub(crate) fn generated_attributes(
    previous: Option<&Asset>,
    bytes: &[u8],
) -> BTreeMap<String, String> {
    let mut attributes = previous
        .map(|asset| asset.metadata.attributes.clone())
        .unwrap_or_default();
    attributes.remove(REPORT);
    let baseline = if let Some(previous) = previous.filter(|asset| protected(asset)) {
        previous
            .metadata
            .attributes
            .get(BASELINE)
            .cloned()
            .unwrap_or_else(|| "hand-edited".into())
    } else {
        BlobId::for_bytes(bytes).to_string()
    };
    attributes.insert(BASELINE.into(), baseline);
    attributes
}

pub struct CleanupGeometry {
    pub findings: Vec<Bounds>,
    pub retry_mask: GrayImage,
}

/// A detector-confirmed hit is leftover text only when at least one of its
/// pixels' bounds overlaps a grown, previously cleaned area. Retry coverage
/// never extends outside that area, even when a hit spans unrelated artwork.
/// The detector's configured confidence threshold remains authoritative.
pub fn cleanup_geometry(cleaned: &GrayImage, hits: &[Bounds], growth: u8) -> CleanupGeometry {
    let (width, height) = cleaned.dimensions();
    if width == 0 || height == 0 {
        return CleanupGeometry {
            findings: Vec::new(),
            retry_mask: cleaned.clone(),
        };
    }
    let allowed = if growth == 0 {
        cleaned.clone()
    } else {
        dilate(cleaned, Norm::LInf, growth)
    };
    let mut retry_mask = GrayImage::new(width, height);
    let mut findings = Vec::new();
    for hit in hits {
        let Some([left, top, right, bottom]) = pixels(*hit, width, height) else {
            continue;
        };
        let overlaps =
            (top..bottom).any(|y| (left..right).any(|x| allowed.get_pixel(x, y)[0] != 0));
        if !overlaps {
            continue;
        }
        findings.push(Bounds {
            x: f64::from(left),
            y: f64::from(top),
            width: f64::from(right - left),
            height: f64::from(bottom - top),
        });
        let radius = u32::from(growth);
        for y in top.saturating_sub(radius)..bottom.saturating_add(radius).min(height) {
            for x in left.saturating_sub(radius)..right.saturating_add(radius).min(width) {
                if allowed.get_pixel(x, y)[0] != 0 {
                    retry_mask.put_pixel(x, y, Luma([255]));
                }
            }
        }
    }
    CleanupGeometry {
        findings,
        retry_mask,
    }
}

fn pixels(bounds: Bounds, width: u32, height: u32) -> Option<[u32; 4]> {
    let right = bounds.x + bounds.width;
    let bottom = bounds.y + bounds.height;
    if ![bounds.x, bounds.y, bounds.width, bounds.height, right, bottom]
        .into_iter()
        .all(f64::is_finite)
        || bounds.width <= 0.0
        || bounds.height <= 0.0
    {
        return None;
    }
    let rect = [
        bounds.x.floor().clamp(0.0, f64::from(width)) as u32,
        bounds.y.floor().clamp(0.0, f64::from(height)) as u32,
        right.ceil().clamp(0.0, f64::from(width)) as u32,
        bottom.ceil().clamp(0.0, f64::from(height)) as u32,
    ];
    (rect[0] < rect[2] && rect[1] < rect[3]).then_some(rect)
}

fn text_hits(output: koharu_ml::koharu_layout_rfdetr_seg_2xl::KoharuLayoutDetections) -> Vec<Bounds> {
    output
        .detections
        .into_iter()
        .filter(|detection| detection.label == "text")
        .map(|detection| {
            let [left, top, right, bottom] = detection.bbox;
            Bounds {
                x: f64::from(left),
                y: f64::from(top),
                width: f64::from(right - left),
                height: f64::from(bottom - top),
            }
        })
        .collect()
}

fn composite(original: &DynamicImage, cleanup: &RgbaImage) -> Arc<DynamicImage> {
    let mut image = original.to_rgba8();
    image::imageops::overlay(&mut image, cleanup, 0, 0);
    Arc::new(DynamicImage::ImageRgba8(image))
}

pub(crate) async fn run_page(
    runner: &StageRunner,
    input: StageInput,
    stop: &StopToken,
    progress: Option<&ProgressSink>,
) -> Result<Option<Patch>> {
    if stop.stopped() {
        return Ok(None);
    }
    let snapshot = &input.scene;
    let page = input.page;
    let mut cleanup_layer = None;
    for entity in snapshot.children(page)? {
        if snapshot
            .component::<RasterLayer>(entity)?
            .is_some_and(|layer| layer.kind == RasterLayerKind::Cleanup)
        {
            cleanup_layer = Some(entity);
            break;
        }
    }
    let Some(layer) = cleanup_layer else {
        return Ok(Some(snapshot.edit().finish()?));
    };
    let source_role = AssetRole::new("source")?;
    let Some(asset) = snapshot.asset(layer, &source_role)? else {
        return Ok(Some(snapshot.edit().finish()?));
    };
    let original = input
        .images
        .get(snapshot, page, "source")
        .await?
        .context("page has no original image")?;
    let cleanup = input
        .images
        .get(snapshot, layer, "source")
        .await?
        .context("cleanup has no image")?
        .to_rgba8();
    ensure!(
        original.dimensions() == cleanup.dimensions(),
        "cleanup dimensions do not match original"
    );
    let mut cleaned = GrayImage::new(cleanup.width(), cleanup.height());
    let mut has_mask = false;
    for role in ["text-mask", "coo-mask"] {
        if let Some(mask) = input.images.get(snapshot, page, role).await? {
            ensure!(
                mask.dimensions() == cleanup.dimensions(),
                "cleanup mask dimensions do not match original"
            );
            has_mask = true;
            for (target, pixel) in cleaned.iter_mut().zip(mask.to_luma8().as_raw()) {
                *target = (*target).max(*pixel);
            }
        }
    }
    for (target, pixel) in cleaned.iter_mut().zip(cleanup.pixels()) {
        *target = if pixel[3] != 0 && (!has_mask || *target != 0) {
            255
        } else {
            0
        };
    }
    let settings = koharu_config::load::<CleanupSettings>("cleanup")?
        .read()?
        .clone();
    let growth = ((cleanup.width().max(cleanup.height()) as f32 / 1024.0) * 6.0)
        .round()
        .clamp(2.0, 32.0) as u8;
    let is_protected = protected(&asset);
    let already_retried =
        stored_result(snapshot, page, layer, &asset)?.is_some_and(|result| result.retried);
    let mut retried = already_retried;
    let mut viewed = snapshot.clone();
    let findings = if cleaned.as_raw().iter().any(|value| *value != 0) {
        let Some(output) = runner
            .cleanup_detection(composite(&original, &cleanup), page, stop, progress)
            .await?
        else {
            return Ok(None);
        };
        let geometry = cleanup_geometry(&cleaned, &text_hits(output), growth);
        if !geometry.findings.is_empty() && !is_protected && !already_retried {
            let mut png = Cursor::new(Vec::new());
            DynamicImage::ImageLuma8(geometry.retry_mask).write_to(&mut png, ImageFormat::Png)?;
            let retry_input = StageInput::new(
                snapshot.clone(),
                page,
                None,
                None,
                input.images.clone(),
                Some(InpaintingMask {
                    page,
                    png: Arc::from(png.into_inner()),
                }),
            )
            .with_cleanup_retry();
            let Some(patch) = runner
                .cleanup_retry(retry_input, settings.second_try_model, stop, progress)
                .await?
            else {
                return Ok(None);
            };
            viewed = snapshot.preview([&patch])?;
            retried = true;
            let after = input
                .images
                .get(&viewed, layer, "source")
                .await?
                .context("retry has no cleanup image")?
                .to_rgba8();
            let Some(output) = runner
                .cleanup_detection(composite(&original, &after), page, stop, progress)
                .await?
            else {
                return Ok(None);
            };
            cleanup_geometry(&cleaned, &text_hits(output), growth).findings
        } else {
            geometry.findings
        }
    } else {
        Vec::new()
    };
    if stop.stopped() {
        return Ok(None);
    }
    result_patch(
        snapshot,
        &viewed,
        CleanupCheckResult {
            page,
            layer,
            findings,
            retried,
            protected: is_protected,
        },
    )
    .await
    .map(Some)
}

async fn result_patch(
    snapshot: &Snapshot,
    viewed: &Snapshot,
    result: CleanupCheckResult,
) -> Result<Patch> {
    let CleanupCheckResult {
        page,
        layer,
        protected: is_protected,
        ..
    } = result;
    let source_role = AssetRole::new("source")?;
    let after = viewed
        .asset(layer, &source_role)?
        .context("cleanup image disappeared")?;
    let bytes = viewed.read_blob(after.blob).await?;
    let stored = StoredResult {
        result,
        original: blob(snapshot, page, "source")?.context("page has no source asset")?,
        cleanup: after.blob,
        text_mask: blob(snapshot, page, "text-mask")?,
        coo_mask: blob(snapshot, page, "coo-mask")?,
    };
    let mut metadata = after.metadata;
    // Metadata-only checks preserve both historical hand work and known
    // generated pixels, independently of the asset ownership used by this edit.
    metadata.attributes.entry(BASELINE.into()).or_insert_with(|| {
        if is_protected {
            "hand-edited".into()
        } else {
            after.blob.to_string()
        }
    });
    metadata
        .attributes
        .insert(REPORT.into(), serde_json::to_string(&stored)?);
    // Explicit cleanup can have a user-owned asset even when its pixels came
    // entirely from AI. Baseline verification grants this bounded edit; scene
    // generation ownership must not be used to replace a user-owned asset.
    let mut edit = snapshot.edit();
    edit.observe_subtree(page)?;
    edit.set_asset(
        layer,
        &source_role,
        AssetInput::new(Arc::<[u8]>::from(bytes.as_ref()), after.media_type, metadata),
    )?;
    Ok(edit.finish()?.with_label("Check cleanup: leftover text"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Bounds {
        Bounds { x, y, width, height }
    }

    #[test]
    fn disjoint_patterns_are_ignored_and_growth_is_clipped_to_cleaned_areas() {
        let mut cleaned = GrayImage::new(40, 30);
        for y in 10..15 {
            for x in 10..15 {
                cleaned.put_pixel(x, y, Luma([255]));
            }
        }
        let geometry = cleanup_geometry(
            &cleaned,
            &[rect(15.0, 11.0, 10.0, 2.0), rect(30.0, 20.0, 3.0, 3.0)],
            2,
        );
        assert_eq!(geometry.findings.len(), 1);
        assert_eq!(geometry.retry_mask.get_pixel(16, 11)[0], 255);
        assert_eq!(geometry.retry_mask.get_pixel(17, 11)[0], 0);
        assert_eq!(geometry.retry_mask.get_pixel(30, 20)[0], 0);
        assert_eq!(geometry.retry_mask.get_pixel(9, 9)[0], 0);
    }

    #[test]
    fn growth_at_page_edge_and_invalid_detector_bounds_are_safe() {
        let mut cleaned = GrayImage::new(8, 8);
        cleaned.put_pixel(0, 0, Luma([255]));
        let geometry = cleanup_geometry(
            &cleaned,
            &[
                rect(-2.0, -2.0, 4.0, 4.0),
                rect(f64::NAN, 0.0, 2.0, 2.0),
                rect(0.0, 0.0, -1.0, 2.0),
            ],
            2,
        );
        assert_eq!(geometry.findings, vec![rect(0.0, 0.0, 2.0, 2.0)]);
        assert_eq!(
            geometry.retry_mask.pixels().filter(|pixel| pixel[0] != 0).count(),
            9,
        );
        let empty = cleanup_geometry(&GrayImage::new(0, 0), &[rect(0.0, 0.0, 1.0, 1.0)], 2);
        assert!(empty.findings.is_empty());
        assert!(empty.retry_mask.as_raw().is_empty());
    }

    #[test]
    fn later_ai_removal_cannot_certify_previously_hand_painted_pixels() {
        let mut asset = Asset {
            origin: Origin::User,
            blob: BlobId::for_bytes(b"painted"),
            media_type: "image/png".into(),
            metadata: koharu_scene::AssetMetadata {
                width: Some(1),
                height: Some(1),
                attributes: BTreeMap::new(),
            },
        };
        assert!(protected(&asset));
        asset.metadata.attributes = generated_attributes(Some(&asset), b"later AI cleanup");
        asset.blob = BlobId::for_bytes(b"later AI cleanup");
        assert!(protected(&asset));
        asset.metadata.attributes = generated_attributes(None, b"automatic");
        asset.blob = BlobId::for_bytes(b"automatic");
        assert!(!protected(&asset));
        asset.blob = BlobId::for_bytes(b"hand stroke");
        assert!(protected(&asset));
        let attributes = generated_attributes(Some(&asset), b"another removal");
        assert_eq!(
            attributes[BASELINE],
            BlobId::for_bytes(b"automatic").to_string(),
        );
    }

    fn image_asset(bytes: &'static [u8]) -> AssetInput {
        AssetInput::new(
            bytes,
            "image/png",
            koharu_scene::AssetMetadata {
                width: Some(8),
                height: Some(8),
                attributes: generated_attributes(None, bytes),
            },
        )
    }

    async fn scene() -> (koharu_scene::Session, EntityId, EntityId) {
        let mut session = koharu_scene::Session::memory().await.unwrap();
        let mut edit = session.snapshot().edit();
        let page = edit
            .add_page(
                koharu_scene::PageDraft::new("page", 8.0, 8.0),
                koharu_scene::At::End,
            )
            .unwrap();
        let layer = edit.add_entity(page, koharu_scene::At::Start).unwrap();
        edit.set(
            layer,
            &RasterLayer {
                origin: Origin::User,
                name: "Cleanup".into(),
                kind: RasterLayerKind::Cleanup,
            },
        )
        .unwrap();
        let source = AssetRole::new("source").unwrap();
        edit.set_asset(page, &source, image_asset(b"original"))
            .unwrap();
        edit.set_asset(layer, &source, image_asset(b"before retry"))
            .unwrap();
        session.commit(edit.finish().unwrap()).await.unwrap();
        (session, page, layer)
    }

    #[tokio::test]
    async fn retry_pixels_and_findings_are_one_undo_step_and_stale_work_is_rejected() {
        let (mut session, page, layer) = scene().await;
        let before = session.snapshot();
        let source = AssetRole::new("source").unwrap();
        let mut retry = before.edit();
        retry
            .set_asset(layer, &source, image_asset(b"after retry"))
            .unwrap();
        let viewed = before.preview([&retry.finish().unwrap()]).unwrap();
        let result = CleanupCheckResult {
            page,
            layer,
            findings: vec![rect(1.0, 1.0, 2.0, 2.0)],
            retried: true,
            protected: false,
        };
        let patch = result_patch(&before, &viewed, result).await.unwrap();
        let mut hand_edit = before.edit();
        hand_edit
            .set_asset(layer, &source, image_asset(b"hand work"))
            .unwrap();
        let changed = before.preview([&hand_edit.finish().unwrap()]).unwrap();
        assert!(patch.rebase_on(&changed).is_err());

        let commit = session.commit(patch).await.unwrap();
        assert_eq!(results(&session.snapshot()).unwrap()[0].findings.len(), 1);
        assert_eq!(
            blob(&session.snapshot(), layer, "source").unwrap(),
            Some(BlobId::for_bytes(b"after retry")),
        );
        session.undo(commit.revision).await.unwrap();
        assert!(results(&session.snapshot()).unwrap().is_empty());
        assert_eq!(
            blob(&session.snapshot(), layer, "source").unwrap(),
            Some(BlobId::for_bytes(b"before retry")),
        );
    }

    #[tokio::test]
    async fn reports_invalidate_on_original_masks_or_cleanup_changes() {
        let (mut session, page, layer) = scene().await;
        let snapshot = session.snapshot();
        let patch = result_patch(
            &snapshot,
            &snapshot,
            CleanupCheckResult {
                page,
                layer,
                findings: vec![],
                retried: false,
                protected: false,
            },
        )
        .await
        .unwrap();
        session.commit(patch).await.unwrap();
        let checked = session.snapshot();
        assert_eq!(results(&checked).unwrap().len(), 1);
        for (entity, role) in [
            (page, "source"),
            (page, "text-mask"),
            (page, "coo-mask"),
            (layer, "source"),
        ] {
            let mut edit = checked.edit();
            // Preserve stale attributes deliberately: blob dependencies, rather
            // than every editing caller clearing a cache, enforce freshness.
            let mut image = image_asset(b"changed");
            if let Some(previous) = checked.asset(entity, &AssetRole::new(role).unwrap()).unwrap() {
                image.metadata.attributes = previous.metadata.attributes;
            }
            edit.set_asset(entity, &AssetRole::new(role).unwrap(), image)
                .unwrap();
            let changed = checked.preview([&edit.finish().unwrap()]).unwrap();
            assert!(
                results(&changed).unwrap().is_empty(),
                "{role} change kept stale results",
            );
        }
    }

    #[tokio::test]
    async fn metadata_only_checks_preserve_historical_pixel_authorship() {
        for generated in [false, true] {
            let (mut session, page, layer) = scene().await;
            let source = AssetRole::new("source").unwrap();
            let mut remove = session.snapshot().edit();
            remove.remove_asset(layer, &source).unwrap();
            session.commit(remove.finish().unwrap()).await.unwrap();
            let mut edit = if generated {
                session.snapshot().edit_as(koharu_scene::Generation::new(
                    koharu_scene::ProducerId::new("dev.koharu.pipeline.inpainting").unwrap(),
                ))
            } else {
                session.snapshot().edit()
            };
            let mut image = image_asset(b"historical pixels");
            image.metadata.attributes.clear();
            edit.set_asset(layer, &source, image).unwrap();
            session.commit(edit.finish().unwrap()).await.unwrap();
            let before = session.snapshot();
            let asset = before.asset(layer, &source).unwrap().unwrap();
            assert_eq!(protected(&asset), !generated);
            let patch = result_patch(
                &before,
                &before,
                CleanupCheckResult {
                    page,
                    layer,
                    findings: vec![],
                    retried: false,
                    protected: !generated,
                },
            )
            .await
            .unwrap();
            session.commit(patch).await.unwrap();
            let after = session.snapshot().asset(layer, &source).unwrap().unwrap();
            assert_eq!(asset.blob, after.blob);
            assert_eq!(protected(&after), !generated);
        }
    }
}
