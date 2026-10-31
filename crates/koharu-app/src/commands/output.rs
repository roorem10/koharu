use anyhow::{Context as _, Result};
use futures::future::try_join_all;
use image::{
    ExtendedColorType, ImageEncoder as _,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};
use koharu_psd::{PsdExportOptions, export_page};
use koharu_rasterizer::{Raster, RasterOptions, Rasterizer};
use koharu_renderer::{Frame, Renderer};
use koharu_scene::{AssetRole, EntityId, Snapshot};
use rayon::prelude::*;
use serde::Deserialize;
use specta::Type;
use std::{io::Write as _, sync::Arc};
use tauri::{State, WebviewWindow, ipc::IpcResponse};
use tauri_runtime_cef::CefRuntime;

use super::{Error, project::CurrentProject};
use koharu_desktop::Desktop;

const THUMBNAIL_EDGE: u32 = 128;

#[derive(Type)]
#[specta(transparent)]
pub(crate) struct ThumbnailBytes(#[specta(type = Vec<u8>)] Vec<u8>);

impl IpcResponse for ThumbnailBytes {
    fn body(self) -> tauri::Result<tauri::ipc::InvokeResponseBody> {
        Ok(self.0.into())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Png,
    Psd,
    Cbz,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn export(
    window: WebviewWindow<CefRuntime>,
    format: ExportFormat,
    project: State<'_, CurrentProject>,
    desktop: State<'_, Desktop>,
) -> std::result::Result<(), Error> {
    Ok(export_selection(window, format, None, &project, &desktop).await?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn export_selected_pages(
    window: WebviewWindow<CefRuntime>,
    format: ExportFormat,
    pages: Vec<EntityId>,
    project: State<'_, CurrentProject>,
    desktop: State<'_, Desktop>,
) -> std::result::Result<(), Error> {
    Ok(export_selection(window, format, Some(pages), &project, &desktop).await?)
}

async fn export_selection(
    window: WebviewWindow<CefRuntime>,
    format: ExportFormat,
    selected: Option<Vec<EntityId>>,
    project: &CurrentProject,
    desktop: &Desktop,
) -> Result<()> {
    let (name, snapshot) = {
        let project = project.project.lock().await;
        let project = project.as_ref().context("no project is open")?;
        (project.name.clone(), project.snapshot())
    };
    let all_pages = snapshot.pages().map(|page| page.id()).collect::<Vec<_>>();
    if let Some(selected) = &selected {
        anyhow::ensure!(
            selected.iter().all(|page| all_pages.contains(page)),
            "a selected page no longer exists"
        );
    }
    let extension = match format {
        ExportFormat::Png | ExportFormat::Cbz => "png",
        ExportFormat::Psd => "psd",
    };
    let labels = all_pages
        .iter()
        .map(|&id| Ok(snapshot.page(id)?.page()?.label.clone()))
        .collect::<Result<Vec<_>>>()?;
    let settings = super::files::load_settings()?;
    let all_names = super::files::names::export_names(
        &settings.export_name_pattern,
        &name,
        &labels.iter().map(String::as_str).collect::<Vec<_>>(),
        extension,
    )?;
    let (pages, names): (Vec<_>, Vec<_>) = all_pages
        .into_iter()
        .zip(all_names)
        .filter(|(page, _)| {
            selected
                .as_ref()
                .is_none_or(|selected| selected.contains(page))
        })
        .unzip();
    if pages.is_empty() {
        return Err(anyhow::anyhow!("there are no pages to export"));
    }
    let dialog = rfd::AsyncFileDialog::new().set_parent(&window);
    let destination = match format {
        ExportFormat::Png | ExportFormat::Psd => dialog.pick_folder().await,
        ExportFormat::Cbz => {
            dialog
                .add_filter("Comic Book Archive", &["cbz"])
                .set_file_name(format!("{name}.cbz"))
                .save_file()
                .await
        }
    };
    let Some(destination) = destination.map(|destination| destination.path().to_owned()) else {
        return Ok(());
    };
    let renderer = desktop.renderer();
    let rasterizer = desktop.rasterizer().await?;
    let mut archive = if matches!(format, ExportFormat::Cbz) {
        let directory = destination.parent().context("archive path has no parent")?;
        Some(zip::ZipWriter::new(tempfile::NamedTempFile::new_in(
            directory,
        )?))
    } else {
        None
    };
    let psd_options = PsdExportOptions::default();
    // Render, encode and write a bounded batch at a time. Preparing every page before writing
    // any kept the whole project's frames and encoded images in memory at once (koharu#1287).
    // Batches run in project order, so file names and archive order are unchanged.
    for (batch, chunk) in pages.chunks(EXPORT_BATCH_PAGES).enumerate() {
        let frames =
            try_join_all(chunk.iter().map(|&page| renderer.render(&snapshot, page))).await?;
        let images = match format {
            ExportFormat::Png | ExportFormat::Cbz => {
                let rasterizer = Arc::clone(&rasterizer);
                tokio_rayon::spawn(move || {
                    frames
                        .par_iter()
                        .map(|frame| encode_png(&rasterizer, frame))
                        .collect::<Result<Vec<_>>>()
                })
                .await?
            }
            ExportFormat::Psd => {
                try_join_all(frames.iter().map(|frame| {
                    export_page(Arc::clone(&rasterizer), &snapshot, frame, &psd_options)
                }))
                .await?
            }
        };
        let start = batch * EXPORT_BATCH_PAGES;
        let batch_names = names[start..start + images.len()].to_vec();
        let directory = destination.clone();
        archive = tokio_rayon::spawn(move || -> Result<_> {
            for (name, bytes) in batch_names.into_iter().zip(images) {
                if let Some(archive) = &mut archive {
                    // PNG data is already compressed.
                    let options = zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored);
                    archive.start_file(name, options)?;
                    archive.write_all(&bytes)?;
                } else {
                    std::fs::write(directory.join(name), bytes)?;
                }
            }
            Ok(archive)
        })
        .await?;
    }
    if let Some(archive) = archive {
        tokio_rayon::spawn(move || -> Result<()> {
            archive.finish()?.persist(destination)?;
            Ok(())
        })
        .await?;
    }
    Ok(())
}

/// Pages prepared, encoded and written together during export. This bounds how many pages are
/// held at once, not their bytes: four 6000 x 9000 RGBA pages alone are about 0.8 GiB before
/// frames, GPU resources and encoder buffers. Lower it for very large pages or PSD exports.
const EXPORT_BATCH_PAGES: usize = 4;

fn encode_png(rasterizer: &Rasterizer, frame: &Frame) -> Result<Vec<u8>> {
    let image = rasterizer
        .rasterize(&frame.raster_frame()?, RasterOptions::default())?
        .image;
    let mut bytes = Vec::new();
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Best, FilterType::Adaptive)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgba8,
        )?;
    Ok(bytes)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_thumbnail(
    page: EntityId,
    project: State<'_, CurrentProject>,
) -> std::result::Result<ThumbnailBytes, Error> {
    let snapshot = project
        .project
        .lock()
        .await
        .as_ref()
        .context("no project is open")?
        .snapshot();
    snapshot.page(page)?;
    let blob = snapshot
        .asset(page, &AssetRole::new("source")?)?
        .with_context(|| format!("page {page} has no source image"))?
        .blob;
    let bytes = snapshot.read_blob(blob).await?;
    let bytes = tokio_rayon::spawn(move || -> Result<Vec<u8>> {
        let image = image::load_from_memory(&bytes).context("failed to decode source image")?;
        if image.width() == 0 || image.height() == 0 {
            return Err(anyhow::anyhow!("source image is empty"));
        }
        let image = image.thumbnail(THUMBNAIL_EDGE, THUMBNAIL_EDGE).to_rgba8();
        let encoder = webp::Encoder::from_rgba(image.as_raw(), image.width(), image.height());
        Ok(encoder.encode(80.0).to_vec())
    })
    .await?;
    Ok(ThumbnailBytes(bytes))
}

pub(crate) async fn rendered_preview(
    renderer: &Renderer,
    rasterizer: Arc<Rasterizer>,
    snapshot: &Snapshot,
    page: EntityId,
) -> Result<Vec<u8>> {
    snapshot.page(page)?;
    let frame = renderer.render(snapshot, page).await?;
    let image = rasterize(rasterizer, &frame, RasterOptions::default())
        .await?
        .image;
    tokio_rayon::spawn(move || {
        let image = image::DynamicImage::ImageRgba8(image)
            .resize(1024, 1024, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        let encoder = webp::Encoder::from_rgba(image.as_raw(), image.width(), image.height());
        Ok::<_, anyhow::Error>(encoder.encode(85.0).to_vec())
    })
    .await
}

async fn rasterize(
    rasterizer: Arc<Rasterizer>,
    frame: &Frame,
    options: RasterOptions,
) -> Result<Raster> {
    let frame = frame.raster_frame()?;
    tokio_rayon::spawn(move || rasterizer.rasterize(&frame, options))
        .await
        .map_err(Into::into)
}
