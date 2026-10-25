use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context as _, Result, bail};
use image::{ImageFormat, ImageReader};
use rayon::prelude::*;
use strum::{EnumIter, EnumMessage, EnumString};
use walkdir::WalkDir;

mod pdf;
mod rar;
mod zip;

#[derive(Clone, Copy, EnumIter, EnumMessage, EnumString)]
#[strum(ascii_case_insensitive)]
pub(super) enum Format {
    #[strum(
        serialize = "png",
        serialize = "jpg",
        serialize = "jpeg",
        serialize = "webp"
    )]
    Raster,
    #[strum(serialize = "cbz", serialize = "zip")]
    Zip,
    #[strum(serialize = "rar")]
    Rar,
    #[strum(serialize = "pdf")]
    Pdf,
}

#[derive(Debug)]
pub(super) struct EncodedPage {
    pub(super) name: String,
    pub(super) bytes: Vec<u8>,
}

pub(super) struct Page {
    pub(super) name: String,
    pub(super) bytes: Arc<[u8]>,
    pub(super) format: ImageFormat,
    pub(super) width: u32,
    pub(super) height: u32,
}

pub(super) struct Selection {
    pub(super) paths: Vec<PathBuf>,
    pub(super) skipped: u32,
}

/// Collect native paths without following file or directory symlinks. The drop
/// limit includes unsupported files so a large unrelated tree is bounded too.
pub(super) fn collect_paths(roots: Vec<PathBuf>, maximum: Option<usize>) -> Result<Selection> {
    let mut selection = Selection {
        paths: Vec::new(),
        skipped: 0,
    };
    let mut inspected = 0;
    let mut consider = |path: PathBuf, regular_file: bool| -> Result<()> {
        inspected += 1;
        if let Some(maximum) = maximum {
            anyhow::ensure!(
                inspected <= maximum,
                "at most {maximum} files can be imported in one drop"
            );
        }
        if regular_file
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.parse::<Format>().is_ok())
        {
            selection.paths.push(path);
        } else {
            selection.skipped += 1;
        }
        Ok(())
    };

    for root in roots {
        if !root.is_absolute() {
            consider(root, false)?;
            continue;
        }
        let metadata = match fs::symlink_metadata(&root) {
            Ok(metadata) => metadata,
            Err(error) => {
                tracing::warn!(%error, path = %root.display(), "could not inspect an import path");
                consider(root, false)?;
                continue;
            }
        };
        if !metadata.is_dir() {
            consider(root, metadata.is_file())?;
            continue;
        }
        for entry in WalkDir::new(root).follow_links(false) {
            match entry {
                Ok(entry) if entry.file_type().is_dir() => {}
                Ok(entry) => {
                    let regular_file = entry.file_type().is_file();
                    consider(entry.into_path(), regular_file)?;
                }
                Err(error) => {
                    tracing::warn!(%error, "could not inspect an import directory entry");
                    consider(PathBuf::new(), false)?;
                }
            }
        }
    }
    anyhow::ensure!(
        !selection.paths.is_empty(),
        "no supported images were found in the selection"
    );
    Ok(selection)
}

fn decode(path: &Path, source: EncodedPage) -> Result<Page> {
    let EncodedPage { name, bytes } = source;
    let format = image::guess_format(&bytes).with_context(|| {
        format!(
            "failed to identify imported image {} ({name})",
            path.display()
        )
    })?;
    let (width, height) = ImageReader::with_format(Cursor::new(bytes.as_slice()), format)
        .into_dimensions()
        .with_context(|| {
            format!(
                "failed to read dimensions of imported image {} ({name})",
                path.display()
            )
        })?;
    Ok(Page {
        name,
        bytes: Arc::<[u8]>::from(bytes),
        format,
        width,
        height,
    })
}

pub(super) fn import(mut paths: Vec<PathBuf>) -> Result<Vec<Page>> {
    alphanumeric_sort::sort_slice_by_os_str_key(&mut paths, |path| {
        path.file_name().unwrap_or_else(|| path.as_os_str())
    });
    let mut groups = paths
        .into_par_iter()
        .map(|path| -> Result<Vec<Page>> {
            let extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .and_then(|extension| extension.parse::<Format>().ok());
            let encoded = match extension {
                Some(Format::Raster) => vec![EncodedPage {
                    name: path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "page".to_owned()),
                    bytes: fs::read(&path)
                        .with_context(|| format!("failed to read {}", path.display()))?,
                }],
                Some(Format::Zip) => zip::extract(&path)?,
                Some(Format::Rar) => rar::extract(&path)?,
                Some(Format::Pdf) => pdf::render(&path)?,
                None => bail!("unsupported page import path {}", path.display()),
            };
            encoded
                .into_iter()
                .map(|source| decode(&path, source))
                .collect()
        })
        .collect::<Result<Vec<_>>>()?;
    let page_count = groups.iter().map(Vec::len).sum();
    let mut pages = Vec::with_capacity(page_count);
    for group in &mut groups {
        pages.append(group);
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_collection_walks_folders_and_counts_unsupported_items() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("nested");
        fs::create_dir(&nested).unwrap();
        let first = directory.path().join("01.PNG");
        let second = nested.join("02.jpg");
        let archive = directory.path().join("chapter.cbz");
        let pdf = directory.path().join("chapter.pdf");
        for path in [&first, &second, &archive, &pdf, &nested.join("notes.txt")] {
            fs::write(path, []).unwrap();
        }
        let selection = collect_paths(vec![directory.path().to_owned()], Some(2_000)).unwrap();
        assert_eq!(selection.skipped, 1);
        let actual = selection
            .paths
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(actual, [first, second, archive, pdf].into_iter().collect());
    }

    #[test]
    fn path_collection_rejects_empty_selections_and_bounds_all_files() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("page.png");
        let notes = directory.path().join("notes.txt");
        fs::write(&image, []).unwrap();
        fs::write(&notes, []).unwrap();
        assert!(collect_paths(vec![notes.clone()], Some(2_000)).is_err());
        assert!(collect_paths(vec![PathBuf::from("page.png")], Some(2_000)).is_err());
        assert!(collect_paths(vec![directory.path().join("missing.png")], Some(2_000)).is_err());
        assert!(collect_paths(Vec::new(), Some(2_000)).is_err());
        let paths = vec![image.clone(), notes, image];
        assert_eq!(
            collect_paths(paths.clone(), Some(3)).unwrap().paths.len(),
            2
        );
        assert!(collect_paths(paths, Some(2)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn path_collection_does_not_follow_root_or_nested_symlinks() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("outside.png"), []).unwrap();
        let image = directory.path().join("inside.png");
        fs::write(&image, []).unwrap();
        let link = directory.path().join("linked-folder");
        symlink(outside.path(), &link).unwrap();
        symlink(
            outside.path().join("outside.png"),
            directory.path().join("linked.png"),
        )
        .unwrap();
        let selection =
            collect_paths(vec![directory.path().to_owned(), link], Some(2_000)).unwrap();
        assert_eq!(selection.paths, [image]);
        assert_eq!(selection.skipped, 3);
    }

    #[test]
    fn top_level_paths_are_naturally_sorted() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "koharu-import-order-{}-{timestamp}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("create fixture directory");
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255]));
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut encoded, ImageFormat::Png)
            .expect("encode fixture");
        let paths = ["page10.PNG", "page2.png", "page1.png"].map(|name| directory.join(name));
        for path in &paths {
            fs::write(path, encoded.get_ref()).expect("write fixture");
        }

        let pages = import(paths.into()).expect("import fixtures");
        fs::remove_dir_all(&directory).expect("remove fixture directory");
        assert_eq!(
            pages
                .iter()
                .map(|page| page.name.as_str())
                .collect::<Vec<_>>(),
            ["page1.png", "page2.png", "page10.PNG"]
        );
    }
}
