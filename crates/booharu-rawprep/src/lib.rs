//! Levels and optional paper whitening before page analysis.

mod levels;

use image::RgbImage;

pub use levels::{DEFAULT_CLIP_PERCENT, LevelsStats};

/// Black and white points after discarding the requested percentage at each end.
#[must_use]
pub fn levels_stats(image: &RgbImage, clip_percent: f64) -> LevelsStats {
    let mut histogram = [0_u64; 256];
    for pixel in image.pixels() {
        histogram[usize::from(levels::luminance(pixel.0))] += 1;
    }
    levels::histogram_stats(&histogram, clip_percent)
}

#[must_use]
pub fn levels_preview_stats(image: &RgbImage) -> LevelsStats {
    levels_stats(image, DEFAULT_CLIP_PERCENT)
}

/// One luminance-derived curve is shared by RGB, rather than balancing channels.
#[must_use]
pub fn auto_levels(image: &RgbImage, clip_percent: f64) -> RgbImage {
    let curve = levels::curve(levels_stats(image, clip_percent));
    let mut output = image.clone();
    for pixel in output.pixels_mut() {
        for channel in &mut pixel.0 {
            *channel = curve[usize::from(*channel)];
        }
    }
    output
}

#[must_use]
pub fn whiten_paper(image: &RgbImage, threshold: u8) -> RgbImage {
    let mut output = image.clone();
    for pixel in output.pixels_mut() {
        if levels::luminance(pixel.0) > threshold {
            pixel.0 = [255; 3];
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use image::Rgb;

    use super::*;

    #[test]
    fn faded_ink_and_grey_paper_reach_black_and_white() {
        let source = RgbImage::from_fn(10, 10, |x, _| Rgb(if x == 0 { [40; 3] } else { [200; 3] }));
        let output = auto_levels(&source, DEFAULT_CLIP_PERCENT);
        assert_eq!(output.dimensions(), source.dimensions());
        assert_eq!(output.get_pixel(0, 0).0, [0; 3]);
        assert_eq!(output.get_pixel(1, 0).0, [255; 3]);
        assert_eq!(
            levels_preview_stats(&source),
            LevelsStats {
                black: 40,
                white: 200
            }
        );
    }

    #[test]
    fn full_range_and_flat_images_are_unchanged() {
        let source = RgbImage::from_fn(256, 1, |x, _| Rgb([x as u8; 3]));
        assert_eq!(auto_levels(&source, 0.0), source);
        let source = RgbImage::from_fn(100, 100, |x, _| {
            Rgb(if x < 2 {
                [0; 3]
            } else if x > 97 {
                [255; 3]
            } else {
                [128; 3]
            })
        });
        assert_eq!(auto_levels(&source, DEFAULT_CLIP_PERCENT), source);
        for value in [0, 128, 200, 255] {
            let blank = RgbImage::from_pixel(4, 4, Rgb([value; 3]));
            assert_eq!(auto_levels(&blank, DEFAULT_CLIP_PERCENT), blank);
        }
        let blank = RgbImage::from_pixel(4, 4, Rgb([170, 180, 190]));
        assert_eq!(auto_levels(&blank, DEFAULT_CLIP_PERCENT), blank);
        assert_eq!(auto_levels(&RgbImage::new(0, 0), 0.5), RgbImage::new(0, 0));
    }

    #[test]
    fn each_tail_is_clipped_including_isolated_scan_noise() {
        let source = RgbImage::from_fn(1000, 1, |x, _| {
            Rgb([match x {
                0 => 0,
                999 => 255,
                1..=100 => 40,
                _ => 200,
            }; 3])
        });
        assert_eq!(
            levels_stats(&source, 0.5),
            LevelsStats {
                black: 40,
                white: 200
            }
        );
        let output = auto_levels(&source, 0.5);
        assert_eq!(output.get_pixel(50, 0).0, [0; 3]);
        assert_eq!(output.get_pixel(500, 0).0, [255; 3]);
    }

    #[test]
    fn screentone_dots_survive_with_greater_contrast() {
        let source = RgbImage::from_fn(16, 16, |x, y| {
            Rgb(if x % 4 == 0 && y % 4 == 0 {
                [80; 3]
            } else {
                [200; 3]
            })
        });
        let output = auto_levels(&source, DEFAULT_CLIP_PERCENT);
        let dots = output.pixels().filter(|pixel| pixel.0 == [0; 3]).count();
        assert_eq!(dots, 16);
        for (before, after) in source.pixels().zip(output.pixels()) {
            assert_eq!(before.0[0] == 80, after.0[0] == 0);
            assert_eq!(before.0[0] == 200, after.0[0] == 255);
        }
        let before_contrast = source.get_pixel(1, 0).0[0] - source.get_pixel(0, 0).0[0];
        let after_contrast = output.get_pixel(1, 0).0[0] - output.get_pixel(0, 0).0[0];
        assert!(after_contrast > before_contrast);
    }

    #[test]
    fn colour_uses_the_same_curve_in_each_channel() {
        let source = RgbImage::from_fn(3, 1, |x, _| {
            Rgb(match x {
                0 => [20; 3],
                1 => [60, 100, 140],
                _ => [220; 3],
            })
        });
        let output = auto_levels(&source, 0.0);
        assert_eq!(output.get_pixel(1, 0).0, [51, 102, 153]);
    }

    #[test]
    fn whitening_is_strictly_above_the_luminance_threshold() {
        let source = RgbImage::from_fn(3, 1, |x, _| {
            Rgb(match x {
                0 => [235; 3],
                1 => [236; 3],
                _ => [255, 0, 0],
            })
        });
        let output = whiten_paper(&source, 235);
        assert_eq!(output.get_pixel(0, 0), source.get_pixel(0, 0));
        assert_eq!(output.get_pixel(1, 0).0, [255; 3]);
        assert_eq!(output.get_pixel(2, 0), source.get_pixel(2, 0));
    }

    #[test]
    fn whitening_follows_levels() {
        let source = RgbImage::from_fn(3, 1, |x, _| {
            Rgb([match x {
                0 => 40,
                1 => 195,
                _ => 200,
            }; 3])
        });
        let prepared = whiten_paper(&auto_levels(&source, 0.0), 235);
        assert_eq!(prepared.get_pixel(0, 0).0, [0; 3]);
        assert_eq!(prepared.get_pixel(1, 0).0, [255; 3]);
        assert_eq!(prepared.get_pixel(2, 0).0, [255; 3]);
        assert_ne!(prepared, auto_levels(&whiten_paper(&source, 235), 0.0));
    }
}
