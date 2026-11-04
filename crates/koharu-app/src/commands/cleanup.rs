//! WO-15: cleanup brushes. `clean_mask` is also used by WO-07.

use std::sync::Arc;

use anyhow::{Context as _, bail, ensure};
use image::{GrayImage, ImageEncoder as _, codecs::png::PngEncoder};
use koharu_pipeline::CleanupSettings;
use koharu_scene::EntityId;
use tauri::{AppHandle, Manager as _, State};
use tauri_runtime_cef::CefRuntime;

use super::{
    Error,
    canvas::Point,
    processing::{JobChannel, JobId, Processing, start_job},
    project::CurrentProject,
};

pub(crate) mod stroke;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, serde::Deserialize, specta::Type)]
pub struct CleanupStroke {
    pub points: Vec<Point>,
    pub diameter: f32,
    pub hardness: f32,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, specta::Type)]
pub struct CleanupRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[tauri::command]
#[specta::specta]
pub(crate) fn get_cleanup_settings() -> Result<CleanupSettings, Error> {
    Ok(koharu_config::load::<CleanupSettings>("cleanup")?
        .read()?
        .clone())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn save_cleanup_settings(settings: CleanupSettings) -> Result<CleanupSettings, Error> {
    if settings.grow_px > 16 {
        return Err(anyhow::anyhow!("cleanup mask growth must be between 0 and 16 pixels").into());
    }
    let config = koharu_config::load::<CleanupSettings>("cleanup")?;
    *config.write()? = settings.clone();
    config.save()?;
    Ok(settings)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn clean_areas(
    page: EntityId,
    strokes: Vec<CleanupStroke>,
    rects: Vec<CleanupRect>,
    handle: AppHandle<CefRuntime>,
    project: State<'_, CurrentProject>,
) -> Result<JobId, Error> {
    let (width, height) = {
        let project = project.project.lock().await;
        let snapshot = project.as_ref().context("no project is open")?.snapshot();
        let value = snapshot.page(page)?.page()?;
        (value.width.round() as u32, value.height.round() as u32)
    };
    let mask =
        tokio_rayon::spawn(move || stroke::areas_mask(width, height, &strokes, &rects)).await?;
    Ok(clean_mask(handle, page, mask).await?)
}

/// Cleans (inpaints) the given mask on one page as one run.
pub(crate) async fn clean_mask(
    handle: AppHandle<CefRuntime>,
    page: EntityId,
    mask: GrayImage,
) -> anyhow::Result<JobId> {
    // Keep failed starts from leaving a mask for an unrelated later run.
    static SUBMISSION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _submission = SUBMISSION.lock().await;
    {
        let project = handle.state::<CurrentProject>();
        let project = project.project.lock().await;
        let snapshot = project.as_ref().context("no project is open")?.snapshot();
        let value = snapshot.page(page)?.page()?;
        ensure!(
            mask.dimensions() == (value.width.round() as u32, value.height.round() as u32),
            "cleanup mask dimensions must match the page"
        );
    }
    let processing = handle.state::<Processing>();
    ensure!(
        processing.stops.lock().is_empty(),
        "another process is already running"
    );
    let (png, bounds) = tokio_rayon::spawn(move || encode(&mask)).await?;
    let png: Arc<[u8]> = Arc::from(png);
    *processing.inpainting_mask.lock() = Some(koharu_pipeline::InpaintingMask {
        page,
        png: png.clone(),
    });
    let result = start_job(
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
    .await;
    if result.is_err() {
        let mut pending = processing.inpainting_mask.lock();
        if pending
            .as_ref()
            .is_some_and(|mask| Arc::ptr_eq(&mask.png, &png))
        {
            *pending = None;
        }
    }
    result
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
mod mask_tests {
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
