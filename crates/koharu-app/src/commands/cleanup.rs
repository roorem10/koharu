//! WO-15: cleanup brushes. `clean_mask` is also used by WO-07.

use std::sync::Arc;

use anyhow::{Context as _, bail};
use image::{GrayImage, ImageEncoder as _, codecs::png::PngEncoder};
use koharu_scene::EntityId;
use tauri::{AppHandle, Manager as _};
use tauri_runtime_cef::CefRuntime;

use super::{
    processing::{JobChannel, JobId, Processing, start_job},
    project::CurrentProject,
};

/// Cleans (inpaints) the given mask on one page as one run.
#[allow(dead_code)] // first callers arrive with WO-07 and WO-15
pub(crate) async fn clean_mask(
    handle: AppHandle<CefRuntime>,
    page: EntityId,
    mask: GrayImage,
) -> anyhow::Result<JobId> {
    let (png, bounds) = tokio_rayon::spawn(move || encode(&mask)).await?;
    *handle.state::<Processing>().inpainting_mask.lock() = Some(koharu_pipeline::InpaintingMask {
        page,
        png: Arc::from(png),
    });
    start_job(
        handle.clone(),
        koharu_pipeline::Scope::Region { page, bounds },
        koharu_pipeline::Operation::Only {
            stage: koharu_pipeline::Stage::Inpainting,
        },
        false,
        &handle.state::<CurrentProject>(),
        &handle.state::<Processing>(),
        &handle.state::<JobChannel>(),
    )
    .await
}

fn encode(mask: &GrayImage) -> anyhow::Result<(Vec<u8>, koharu_pipeline::Bounds)> {
    let (width, height) = mask.dimensions();
    if width == 0 || height == 0 {
        bail!("the cleanup mask is empty");
    }
    let mut dirty = None::<[u32; 4]>;
    for (x, y, pixel) in mask.enumerate_pixels() {
        if pixel.0[0] == 0 {
            continue;
        }
        dirty = Some(match dirty {
            Some([min_x, min_y, max_x, max_y]) => {
                [min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y)]
            }
            None => [x, y, x, y],
        });
    }
    let [left, top, right, bottom] = dirty.context("the cleanup mask has nothing to clean")?;
    let mut png = Vec::new();
    PngEncoder::new(&mut png).write_image(
        mask.as_raw(),
        width,
        height,
        image::ExtendedColorType::L8,
    )?;
    Ok((
        png,
        koharu_pipeline::Bounds {
            x: f64::from(left),
            y: f64::from(top),
            width: f64::from(right - left + 1),
            height: f64::from(bottom - top + 1),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_bounds_cover_every_marked_pixel() {
        let mut mask = GrayImage::new(20, 10);
        mask.put_pixel(3, 2, image::Luma([255]));
        mask.put_pixel(12, 7, image::Luma([40]));
        let (png, bounds) = encode(&mask).unwrap();
        assert!(!png.is_empty());
        assert_eq!(
            (bounds.x, bounds.y, bounds.width, bounds.height),
            (3., 2., 10., 6.)
        );
    }

    #[test]
    fn empty_mask_is_rejected() {
        assert!(encode(&GrayImage::new(4, 4)).is_err());
    }
}
