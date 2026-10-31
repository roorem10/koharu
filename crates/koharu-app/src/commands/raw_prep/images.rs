use std::io::Cursor;

use anyhow::{Context as _, Result};
use booharu_rawprep::{auto_levels, levels_stats, whiten_paper};
use image::{DynamicImage, ImageFormat, RgbImage};

use super::{PrepPreview, RawPrepOptions};

fn apply(source: &RgbImage, options: RawPrepOptions) -> RgbImage {
    let leveled = if options.levels {
        auto_levels(source, options.clip_percent)
    } else {
        source.clone()
    };
    if options.whiten {
        whiten_paper(&leveled, options.threshold)
    } else {
        leveled
    }
}

fn replace_rgb(source: DynamicImage, prepared: RgbImage) -> DynamicImage {
    if !source.color().has_alpha() {
        return DynamicImage::ImageRgb8(prepared);
    }
    let mut rgba = source.into_rgba8();
    for (pixel, colour) in rgba.pixels_mut().zip(prepared.pixels()) {
        pixel.0[..3].copy_from_slice(&colour.0);
    }
    DynamicImage::ImageRgba8(rgba)
}

fn png(image: &DynamicImage) -> Result<Vec<u8>> {
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, ImageFormat::Png)?;
    Ok(output.into_inner())
}

pub(super) fn prepare(bytes: &[u8], options: RawPrepOptions) -> Result<Option<Vec<u8>>> {
    let source = image::load_from_memory(bytes).context("could not decode the page source")?;
    let rgb = source.to_rgb8();
    let prepared = apply(&rgb, options);
    if prepared == rgb {
        return Ok(None);
    }
    Ok(Some(png(&replace_rgb(source, prepared))?))
}

pub(super) fn preview(bytes: &[u8], options: RawPrepOptions) -> Result<PrepPreview> {
    let source = image::load_from_memory(bytes).context("could not decode the page source")?;
    let rgb = source.to_rgb8();
    // Measure and apply at full resolution so tiny screentones do not change the curve.
    let stats = levels_stats(&rgb, options.clip_percent);
    let prepared = replace_rgb(source.clone(), apply(&rgb, options));
    let reduced = |image: DynamicImage| {
        if image.width().max(image.height()) > 800 {
            image.thumbnail(800, 800)
        } else {
            image
        }
    };
    Ok(PrepPreview {
        before: png(&reduced(source))?,
        after: png(&reduced(prepared))?,
        black: stats.black,
        white: stats.white,
    })
}

#[cfg(test)]
mod tests {
    use image::{Rgb, Rgba, RgbaImage};

    use super::*;

    #[test]
    fn png_preparation_preserves_dimensions_and_alpha() {
        let source = RgbaImage::from_fn(4, 2, |x, _| {
            Rgba(if x < 2 {
                [40, 40, 40, 100]
            } else {
                [200, 200, 200, 255]
            })
        });
        let input = png(&DynamicImage::ImageRgba8(source)).unwrap();
        let output = prepare(&input, RawPrepOptions::default()).unwrap().unwrap();
        let output = image::load_from_memory(&output).unwrap().into_rgba8();
        assert_eq!(output.dimensions(), (4, 2));
        assert_eq!(output.get_pixel(0, 0).0, [0, 0, 0, 100]);
        assert_eq!(output.get_pixel(3, 0).0, [255; 4]);
    }

    #[test]
    fn preview_uses_full_resolution_levels_and_never_upscales() {
        let source = RgbImage::from_fn(1600, 1200, |x, y| {
            Rgb(if x % 4 == 0 && y % 4 == 0 {
                [40; 3]
            } else {
                [200; 3]
            })
        });
        let input = png(&DynamicImage::ImageRgb8(source)).unwrap();
        let result = preview(&input, RawPrepOptions::default()).unwrap();
        assert_eq!((result.black, result.white), (40, 200));
        for bytes in [result.before, result.after] {
            let image = image::load_from_memory(&bytes).unwrap();
            assert_eq!((image.width(), image.height()), (800, 600));
        }
        let input = png(&DynamicImage::ImageRgb8(RgbImage::from_pixel(
            2,
            4,
            Rgb([200; 3]),
        )))
        .unwrap();
        let preview = preview(&input, RawPrepOptions::default()).unwrap();
        let image = image::load_from_memory(&preview.after).unwrap();
        assert_eq!((image.width(), image.height()), (2, 4));
    }

    #[test]
    fn options_apply_levels_before_optional_whitening_and_skip_noops() {
        let source = RgbImage::from_fn(3, 1, |x, _| {
            Rgb([match x {
                0 => 40,
                1 => 195,
                _ => 200,
            }; 3])
        });
        let options = RawPrepOptions {
            whiten: true,
            ..Default::default()
        };
        let output = apply(&source, options);
        assert_eq!(output.get_pixel(1, 0).0, [255; 3]);
        let input = png(&DynamicImage::ImageRgb8(source)).unwrap();
        assert!(
            prepare(
                &input,
                RawPrepOptions {
                    levels: false,
                    whiten: false,
                    ..options
                }
            )
            .unwrap()
            .is_none()
        );
        assert!(prepare(b"not an image", options).is_err());
    }
}
