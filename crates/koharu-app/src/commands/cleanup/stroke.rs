use anyhow::{Result, ensure};
use image::{GrayImage, Luma};

use super::{CleanupRect, CleanupStroke, Point};

pub(crate) fn validate(points: &[Point], diameter: f32, hardness: f32) -> Result<()> {
    ensure!(
        diameter.is_finite() && diameter > 0.0 && !points.is_empty(),
        "a stroke requires a positive diameter and at least one point"
    );
    ensure!(
        hardness.is_finite() && (0.0..=100.0).contains(&hardness),
        "brush hardness must be between 0 and 100 percent"
    );
    ensure!(
        points.iter().all(|p| p.x.is_finite() && p.y.is_finite()),
        "stroke points must be finite"
    );
    Ok(())
}

pub(crate) fn add_stroke(mask: &mut GrayImage, points: &[Point], diameter: f32, hardness: f32) {
    let radius = f64::from(diameter) * 0.5;
    let falloff = (radius * (1.0 - f64::from(hardness) / 100.0) - 0.5).max(1.0);
    for (start, end) in points
        .iter()
        .zip(points.iter().skip(1))
        .chain(points.last().map(|point| (point, point)))
    {
        let left = (start.x.min(end.x) - radius - 0.5).floor().max(0.0) as u32;
        let top = (start.y.min(end.y) - radius - 0.5).floor().max(0.0) as u32;
        let right = (start.x.max(end.x) + radius + 0.5)
            .ceil()
            .min(f64::from(mask.width())) as u32;
        let bottom = (start.y.max(end.y) + radius + 0.5)
            .ceil()
            .min(f64::from(mask.height())) as u32;
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length_squared = dx.mul_add(dx, dy * dy);
        for y in top..bottom {
            for x in left..right {
                let px = f64::from(x) + 0.5;
                let py = f64::from(y) + 0.5;
                let progress = if length_squared <= f64::EPSILON {
                    0.0
                } else {
                    (((px - start.x) * dx + (py - start.y) * dy) / length_squared).clamp(0.0, 1.0)
                };
                let distance =
                    (px - (start.x + progress * dx)).hypot(py - (start.y + progress * dy));
                let coverage =
                    (((radius + 0.5 - distance) / falloff).clamp(0.0, 1.0) * 255.0).round() as u8;
                let pixel = mask.get_pixel_mut(x, y);
                pixel[0] = pixel[0].max(coverage);
            }
        }
    }
}

pub(crate) fn areas_mask(
    width: u32,
    height: u32,
    strokes: &[CleanupStroke],
    rects: &[CleanupRect],
) -> Result<GrayImage> {
    ensure!(width > 0 && height > 0, "page dimensions must be positive");
    ensure!(
        !strokes.is_empty() || !rects.is_empty(),
        "there are no cleanup areas"
    );
    let mut mask = GrayImage::new(width, height);
    for stroke in strokes {
        validate(&stroke.points, stroke.diameter, stroke.hardness)?;
        add_stroke(&mut mask, &stroke.points, stroke.diameter, stroke.hardness);
    }
    for rect in rects {
        ensure!(
            [
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                rect.x + rect.width,
                rect.y + rect.height
            ]
            .iter()
            .all(|value| value.is_finite())
                && rect.width > 0.0
                && rect.height > 0.0,
            "cleanup rectangles must have positive, finite dimensions"
        );
        let left = rect.x.floor().max(0.0) as u32;
        let top = rect.y.floor().max(0.0) as u32;
        let right = (rect.x + rect.width).ceil().clamp(0.0, f64::from(width)) as u32;
        let bottom = (rect.y + rect.height).ceil().clamp(0.0, f64::from(height)) as u32;
        for y in top..bottom {
            for x in left..right {
                mask.put_pixel(x, y, Luma([255]));
            }
        }
    }
    Ok(mask)
}
