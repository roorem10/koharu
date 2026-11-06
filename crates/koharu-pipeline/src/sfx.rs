use std::collections::BTreeSet;

use anyhow::{Context as _, Result};
use image::{GrayImage, Luma};
use koharu_scene::{EntityId, Geometry, Origin, Snapshot, Visibility};

/// Resolved by the application after chapter reading, before processing starts.
pub const SFX_ACTION_KEY: &str = "dev.booharu.sfx.action";
pub const SFX_SUBTITLE_SOURCE_KEY: &str = "dev.booharu.sfx.source";

pub(crate) fn pages_requiring_text_before_cleanup(
    snapshot: &Snapshot,
) -> Result<BTreeSet<EntityId>> {
    let mut pages = BTreeSet::new();
    for page in snapshot.pages() {
        if let Some(group) = page.text_group()? {
            for layer in group.text_layers()? {
                // Explicit visibility cannot be restamped when a new SFX
                // choice arrives, so cleanup observes its typography as well.
                if layer
                    .typography()?
                    .is_some_and(|style| style.extensions.contains_key(SFX_ACTION_KEY))
                    || layer
                        .visibility()?
                        .is_some_and(|visibility| visibility.origin == Origin::User)
                {
                    pages.insert(page.id());
                    break;
                }
            }
        }
    }
    Ok(pages)
}

pub(crate) fn page_has_sfx(snapshot: &Snapshot, page: EntityId) -> Result<bool> {
    if let Some(group) = snapshot.page(page)?.text_group()? {
        for layer in group.text_layers()? {
            if layer
                .typography()?
                .is_some_and(|style| style.extensions.contains_key(SFX_ACTION_KEY))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn preserves_original(action: Option<&str>, replaced: bool) -> bool {
    match action {
        None => false,
        Some("translate") => !replaced,
        // Unknown actions fail closed rather than destroying their source artwork.
        Some(_) => true,
    }
}

/// Detection's mask predates the reading labels. Subtract protected source areas
/// here, after combining all masks, so overlapping detections cannot erase SFX.
/// Returns whether the resulting patch must observe these page-level decisions.
pub(crate) fn protect_masks(
    snapshot: &Snapshot,
    page: EntityId,
    mask: &mut GrayImage,
    text_mask: &mut GrayImage,
) -> Result<bool> {
    let Some(group) = snapshot.page(page)?.text_group()? else {
        return Ok(false);
    };
    let mut has_sfx = false;
    for layer in group.text_layers()? {
        let Some(style) = layer.typography()? else {
            continue;
        };
        let action = style.extensions.get(SFX_ACTION_KEY).map(String::as_str);
        if action.is_none() {
            continue;
        }
        has_sfx = true;
        let content = layer.content()?;
        let translated = content
            .translation()?
            .is_some_and(|translation| !translation.text.value.trim().is_empty());
        let mut visible = true;
        let mut ancestor = Some(layer.id());
        while let Some(entity) = ancestor {
            if snapshot
                .component::<Visibility>(entity)?
                .is_some_and(|value| !value.visible || value.opacity <= 0.0)
            {
                visible = false;
                break;
            }
            ancestor = snapshot.parent(entity)?;
        }
        if !preserves_original(action, translated && visible) {
            continue;
        }
        // A moved typesetting frame does not move the original ink on the page.
        let geometry = if let Some(region) = content.source_region()? {
            region.geometry()?
        } else {
            layer
                .frame()?
                .context("protected SFX has no source geometry")?
        };
        exclude_geometry(mask, text_mask, &geometry);
    }
    Ok(has_sfx)
}

fn exclude_geometry(mask: &mut GrayImage, text_mask: &mut GrayImage, geometry: &Geometry) {
    let (width, height) = mask.dimensions();
    // Detection closes a dilated text mask. Protect its two-radius interaction
    // halo as well as the source bounds; preservation wins where boxes overlap.
    let radius = ((width.max(height) as f64 / 1024.0) * 6.0)
        .round()
        .clamp(1.0, 255.0)
        * 2.0;
    let (mut left, mut top) = (f64::INFINITY, f64::INFINITY);
    let (mut right, mut bottom) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for point in &geometry.points {
        left = left.min(point.x);
        top = top.min(point.y);
        right = right.max(point.x);
        bottom = bottom.max(point.y);
    }
    let left = (left - radius).floor().clamp(0.0, f64::from(width)) as u32;
    let top = (top - radius).floor().clamp(0.0, f64::from(height)) as u32;
    let right = (right + radius).ceil().clamp(0.0, f64::from(width)) as u32;
    let bottom = (bottom + radius).ceil().clamp(0.0, f64::from(height)) as u32;
    for y in top..bottom {
        for x in left..right {
            mask.put_pixel(x, y, Luma([0]));
            text_mask.put_pixel(x, y, Luma([0]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_replaced_sfx_can_be_erased() {
        for translated in [false, true] {
            assert!(preserves_original(Some("skip"), translated));
            assert!(preserves_original(Some("subtitle"), translated));
            assert!(preserves_original(Some("unknown"), translated));
            assert!(!preserves_original(None, translated));
        }
        assert!(preserves_original(Some("translate"), false));
        assert!(!preserves_original(Some("translate"), true));
    }

    #[test]
    fn preservation_wins_over_overlapping_masks_and_includes_detection_halo() {
        let mut mask = GrayImage::from_pixel(32, 32, Luma([255]));
        let mut text_mask = mask.clone();
        exclude_geometry(
            &mut mask,
            &mut text_mask,
            &Geometry::rectangle(8.0, 8.0, 8.0, 8.0),
        );
        for image in [&mask, &text_mask] {
            assert_eq!(image.get_pixel(8, 8)[0], 0);
            assert_eq!(image.get_pixel(6, 6)[0], 0);
            assert_eq!(image.get_pixel(17, 17)[0], 0);
            assert_eq!(image.get_pixel(5, 8)[0], 255);
            assert_eq!(image.get_pixel(18, 8)[0], 255);
        }
    }

    #[tokio::test]
    async fn protects_source_ink_after_the_typesetting_frame_moves() {
        use koharu_scene::{
            At, Authored, Origin, PageDraft, RecognizedFrom, Session, TextLayout, TextLayoutKind,
            TextRegion, Translation, Typography,
        };
        let mut session = Session::memory().await.unwrap();
        let mut ids = None;
        let patch = session
            .snapshot()
            .patch(|edit| {
                let page = edit.add_page(PageDraft::new("page", 64.0, 64.0), At::End)?;
                let region = edit.add_analysis_region::<TextRegion>(
                    page,
                    At::End,
                    &Geometry::rectangle(8.0, 8.0, 8.0, 8.0),
                    None,
                )?;
                let content = edit.add_text_content(page, At::End)?;
                edit.relate::<RecognizedFrom>(content, region)?;
                let layer = edit.add_text_layer(
                    page,
                    At::End,
                    content,
                    &TextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: None,
                        warp: None,
                    },
                )?;
                edit.set(layer, &Geometry::rectangle(40.0, 40.0, 8.0, 8.0))?;
                edit.set(
                    layer,
                    &Typography {
                        origin: Origin::User,
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
                        extensions: [(SFX_ACTION_KEY.into(), "translate".into())].into(),
                        stroke_method: None,
                        stroke_position: None,
                        stroke_join: None,
                        outer_stroke_color: None,
                        outer_stroke_width: None,
                        line_spacing: None,
                        letter_spacing: None,
                        fill_gradient: None,
                    },
                )?;
                ids = Some((page, layer, content));
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        let (page, layer, content) = ids.unwrap();
        for (action, translation, visible, opacity, protected) in [
            ("translate", "", true, 1.0, true),
            ("translate", "  ", true, 1.0, true),
            ("translate", "knock", true, 1.0, false),
            ("translate", "knock", false, 1.0, true),
            ("translate", "knock", true, 0.0, true),
            ("skip", "knock", true, 1.0, true),
            ("subtitle", "knock", true, 1.0, true),
        ] {
            let snapshot = session.snapshot();
            let patch = snapshot
                .patch(|edit| {
                    let mut typography = snapshot.text_layer(layer)?.typography()?.unwrap();
                    typography
                        .extensions
                        .insert(SFX_ACTION_KEY.into(), action.into());
                    edit.set(layer, &typography)?;
                    edit.set(
                        layer,
                        &Visibility {
                            origin: Origin::User,
                            visible,
                            opacity,
                        },
                    )?;
                    edit.set(
                        content,
                        &Translation {
                            text: Authored::user(translation.into()),
                            language: None,
                        },
                    )
                })
                .unwrap();
            let snapshot = session.commit(patch).await.unwrap().snapshot;
            let mut mask = GrayImage::from_pixel(64, 64, Luma([255]));
            let mut text_mask = mask.clone();
            assert!(protect_masks(&snapshot, page, &mut mask, &mut text_mask).unwrap());
            assert_eq!(mask.get_pixel(10, 10)[0], if protected { 0 } else { 255 });
            assert_eq!(text_mask.get_pixel(10, 10), mask.get_pixel(10, 10));
            assert_eq!(mask.get_pixel(44, 44)[0], 255);
            assert_eq!(
                pages_requiring_text_before_cleanup(&snapshot).unwrap(),
                BTreeSet::from([page])
            );
        }

        let snapshot = session.snapshot();
        let patch = snapshot
            .patch(|edit| {
                let mut typography = snapshot.text_layer(layer)?.typography()?.unwrap();
                typography
                    .extensions
                    .insert(SFX_ACTION_KEY.into(), "translate".into());
                edit.set(layer, &typography)?;
                edit.set(
                    snapshot.parent(layer)?.unwrap(),
                    &Visibility {
                        origin: Origin::User,
                        visible: false,
                        opacity: 1.0,
                    },
                )
            })
            .unwrap();
        let snapshot = session.commit(patch).await.unwrap().snapshot;
        let mut mask = GrayImage::from_pixel(64, 64, Luma([255]));
        let mut text_mask = mask.clone();
        protect_masks(&snapshot, page, &mut mask, &mut text_mask).unwrap();
        assert_eq!(mask.get_pixel(10, 10)[0], 0);

        let patch = snapshot
            .patch(|edit| {
                let mut typography = snapshot.text_layer(layer)?.typography()?.unwrap();
                typography.extensions.remove(SFX_ACTION_KEY);
                edit.set(layer, &typography)
            })
            .unwrap();
        let snapshot = session.commit(patch).await.unwrap().snapshot;
        assert!(!page_has_sfx(&snapshot, page).unwrap());
        assert_eq!(
            pages_requiring_text_before_cleanup(&snapshot).unwrap(),
            BTreeSet::from([page])
        );
    }
}
