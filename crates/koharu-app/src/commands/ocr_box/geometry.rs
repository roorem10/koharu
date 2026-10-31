use anyhow::{Context as _, Result, ensure};
use image::{GrayImage, Luma};
use koharu_pipeline::Bounds;
use koharu_scene::{EntityId, Geometry, Point};

pub(super) fn extents(geometry: &Geometry) -> Result<Bounds> {
    ensure!(geometry.points.len() >= 3, "box has no polygon");
    let mut left = f64::INFINITY;
    let mut top = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    for point in &geometry.points {
        ensure!(
            point.x.is_finite() && point.y.is_finite(),
            "box coordinates must be finite"
        );
        left = left.min(point.x);
        top = top.min(point.y);
        right = right.max(point.x);
        bottom = bottom.max(point.y);
    }
    ensure!(right > left && bottom > top, "box is empty");
    Ok(Bounds {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

pub(super) fn clip_rect(rect: Bounds, width: f64, height: f64) -> Result<Bounds> {
    ensure!(
        [rect.x, rect.y, rect.width, rect.height]
            .iter()
            .all(|n| n.is_finite())
            && rect.width > 0.
            && rect.height > 0.,
        "rectangle must be finite and nonempty"
    );
    let left = rect.x.max(0.);
    let top = rect.y.max(0.);
    let right = (rect.x + rect.width).min(width);
    let bottom = (rect.y + rect.height).min(height);
    ensure!(
        right > left && bottom > top,
        "rectangle is outside the page"
    );
    Ok(Bounds {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

pub(super) fn rectangle(rect: Bounds) -> Geometry {
    Geometry::rectangle(rect.x, rect.y, rect.width, rect.height)
}

fn area(points: &[Point]) -> f64 {
    if points.len() < 3 {
        return 0.;
    }
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| a.x * b.y - a.y * b.x)
        .sum::<f64>()
        .abs()
        * 0.5
}

// Clip the actual bubble polygon, rather than its bounding box: corners of a round
// bubble and the gap between two bubbles must not count as dialogue containment.
fn intersection_area(points: &[Point], rect: Bounds) -> f64 {
    let mut polygon = points.to_vec();
    for (axis, edge, above) in [
        (0, rect.x, true),
        (0, rect.x + rect.width, false),
        (1, rect.y, true),
        (1, rect.y + rect.height, false),
    ] {
        let input = std::mem::take(&mut polygon);
        let coordinate = |p: Point| if axis == 0 { p.x } else { p.y };
        let inside = |p: Point| {
            if above {
                coordinate(p) >= edge
            } else {
                coordinate(p) <= edge
            }
        };
        let Some(mut previous) = input.last().copied() else {
            return 0.;
        };
        for current in input {
            if inside(current) != inside(previous) {
                let t =
                    (edge - coordinate(previous)) / (coordinate(current) - coordinate(previous));
                polygon.push(Point {
                    x: previous.x + t * (current.x - previous.x),
                    y: previous.y + t * (current.y - previous.y),
                });
            }
            if inside(current) {
                polygon.push(current);
            }
            previous = current;
        }
    }
    area(&polygon)
}

pub(super) fn choose_bubble(bubbles: &[(EntityId, Geometry)], rect: Bounds) -> Option<EntityId> {
    bubbles
        .iter()
        .filter(|(_, geometry)| {
            intersection_area(&geometry.points, rect) >= rect.width * rect.height * 0.9
        })
        .min_by(|(_, a), (_, b)| area(&a.points).total_cmp(&area(&b.points)))
        .map(|(id, _)| *id)
}

pub(super) fn subtitle_rect(
    source: Bounds,
    vertical: bool,
    size: f32,
    width: f64,
    height: f64,
) -> Bounds {
    let subtitle_width = (f64::from(size) * 12.).min(width);
    let subtitle_height = (f64::from(size) * 2.8).min(height);
    Bounds {
        x: (if vertical {
            source.x - subtitle_width - 6.
        } else {
            source.x
        })
        .clamp(0., width - subtitle_width),
        y: (if vertical {
            source.y
        } else {
            source.y + source.height + 6.
        })
        .clamp(0., height - subtitle_height),
        width: subtitle_width,
        height: subtitle_height,
    }
}

fn contains(points: &[Point], x: f64, y: f64) -> bool {
    let mut inside = false;
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        if (a.y > y) != (b.y > y) && x < (b.x - a.x) * (y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

pub(super) fn union_mask(
    width: u32,
    height: u32,
    text: Option<&GrayImage>,
    polygons: &[Geometry],
    grow: u32,
) -> Result<GrayImage> {
    ensure!(width > 0 && height > 0, "page is empty");
    if let Some(text) = text {
        ensure!(
            text.dimensions() == (width, height),
            "page text mask has the wrong dimensions"
        );
    }
    let mut mask = GrayImage::new(width, height);
    for polygon in polygons {
        let bounds = extents(polygon)?;
        let Ok(rect) = clip_rect(bounds, f64::from(width), f64::from(height)) else {
            continue;
        };
        let left = rect.x.floor() as u32;
        let top = rect.y.floor() as u32;
        let right = ((rect.x + rect.width).ceil() as u32).min(width);
        let bottom = ((rect.y + rect.height).ceil() as u32).min(height);
        let mut marked = false;
        for y in top..bottom {
            for x in left..right {
                if contains(&polygon.points, f64::from(x) + 0.5, f64::from(y) + 0.5)
                    && text.is_none_or(|text| text.get_pixel(x, y)[0] != 0)
                {
                    mask.put_pixel(x, y, Luma([255]));
                    marked = true;
                }
            }
        }
        // A missed box has no detector pixels even when the rest of the page has
        // a mask. Its explicitly selected polygon is still a valid cleanup area.
        if text.is_some() && !marked {
            for y in top..bottom {
                for x in left..right {
                    if contains(&polygon.points, f64::from(x) + 0.5, f64::from(y) + 0.5) {
                        mask.put_pixel(x, y, Luma([255]));
                    }
                }
            }
        }
    }
    if grow == 0 {
        return Ok(mask);
    }
    // Separable dilation is linear in page pixels, independent of box count.
    let mut horizontal = GrayImage::new(width, height);
    for y in 0..height {
        let mut count = (0..=grow.min(width - 1))
            .filter(|x| mask.get_pixel(*x, y)[0] != 0)
            .count();
        for x in 0..width {
            if count > 0 {
                horizontal.put_pixel(x, y, Luma([255]));
            }
            if x >= grow && mask.get_pixel(x - grow, y)[0] != 0 {
                count -= 1;
            }
            if x + grow + 1 < width && mask.get_pixel(x + grow + 1, y)[0] != 0 {
                count += 1;
            }
        }
    }
    for x in 0..width {
        let mut count = (0..=grow.min(height - 1))
            .filter(|y| horizontal.get_pixel(x, *y)[0] != 0)
            .count();
        for y in 0..height {
            mask.put_pixel(x, y, Luma([if count > 0 { 255 } else { 0 }]));
            if y >= grow && horizontal.get_pixel(x, y - grow)[0] != 0 {
                count -= 1;
            }
            if y + grow + 1 < height && horizontal.get_pixel(x, y + grow + 1)[0] != 0 {
                count += 1;
            }
        }
    }
    Ok(mask)
}

pub(super) async fn text_mask(
    snapshot: &koharu_scene::Snapshot,
    page: EntityId,
) -> Result<Option<GrayImage>> {
    let Some(asset) = snapshot.asset(page, &koharu_scene::AssetRole::new("text-mask")?)? else {
        return Ok(None);
    };
    let bytes = snapshot.read_blob(asset.blob).await?;
    Ok(Some(
        tokio_rayon::spawn(move || image::load_from_memory(&bytes).map(|image| image.to_luma8()))
            .await
            .context("failed to decode page text mask")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smallest_containing_bubble_wins_but_crossing_bubbles_is_free_text() {
        let large = EntityId::new();
        let small = EntityId::new();
        let other = EntityId::new();
        let bubbles = vec![
            (large, Geometry::rectangle(0., 0., 100., 100.)),
            (small, Geometry::rectangle(10., 10., 30., 60.)),
            (other, Geometry::rectangle(110., 0., 40., 100.)),
        ];
        assert_eq!(
            choose_bubble(
                &bubbles,
                Bounds {
                    x: 15.,
                    y: 15.,
                    width: 10.,
                    height: 40.
                }
            ),
            Some(small)
        );
        assert_eq!(
            choose_bubble(
                &bubbles,
                Bounds {
                    x: 90.,
                    y: 10.,
                    width: 30.,
                    height: 30.
                }
            ),
            None
        );
        assert_eq!(
            choose_bubble(
                &bubbles,
                Bounds {
                    x: 160.,
                    y: 10.,
                    width: 20.,
                    height: 30.
                }
            ),
            None
        );
        assert_eq!(
            choose_bubble(
                &bubbles,
                Bounds {
                    x: 95.,
                    y: 10.,
                    width: 5.,
                    height: 30.
                }
            ),
            Some(large)
        );
        assert_eq!(
            choose_bubble(
                &bubbles,
                Bounds {
                    x: 10.,
                    y: 91.,
                    width: 20.,
                    height: 10.
                }
            ),
            Some(large)
        );
        assert_eq!(
            choose_bubble(
                &bubbles,
                Bounds {
                    x: 10.,
                    y: 91.01,
                    width: 20.,
                    height: 10.
                }
            ),
            None
        );
    }

    #[test]
    fn bounding_box_corners_are_not_inside_a_diamond_bubble() {
        let diamond = Geometry {
            origin: koharu_scene::Origin::User,
            points: vec![
                Point { x: 50., y: 0. },
                Point { x: 100., y: 50. },
                Point { x: 50., y: 100. },
                Point { x: 0., y: 50. },
            ],
        };
        assert!(
            choose_bubble(
                &[(EntityId::new(), diamond)],
                Bounds {
                    x: 0.,
                    y: 0.,
                    width: 20.,
                    height: 20.
                }
            )
            .is_none()
        );
    }

    #[test]
    fn subtitles_stay_on_the_page_at_both_edges() {
        for vertical in [false, true] {
            for source in [
                Bounds {
                    x: 0.,
                    y: 0.,
                    width: 30.,
                    height: 80.,
                },
                Bounds {
                    x: 490.,
                    y: 490.,
                    width: 10.,
                    height: 10.,
                },
            ] {
                let rect = subtitle_rect(source, vertical, 18., 500., 500.);
                assert!(rect.x >= 0. && rect.y >= 0.);
                assert!(rect.x + rect.width <= 500. && rect.y + rect.height <= 500.);
            }
        }
        let source = Bounds {
            x: 300.,
            y: 100.,
            width: 20.,
            height: 80.,
        };
        assert!(subtitle_rect(source, true, 18., 500., 500.).x < source.x);
        assert!(subtitle_rect(source, false, 18., 500., 500.).y > source.y);
    }

    #[test]
    fn redo_mask_is_the_union_and_preserves_the_gap_between_boxes() {
        let boxes = [
            Geometry::rectangle(2., 2., 4., 4.),
            Geometry::rectangle(24., 2., 4., 4.),
        ];
        let mask = union_mask(40, 20, None, &boxes, 4).unwrap();
        for polygon in boxes {
            let single = union_mask(40, 20, None, &[polygon], 4).unwrap();
            for (a, b) in mask.pixels().zip(single.pixels()) {
                if b[0] != 0 {
                    assert_eq!(a[0], 255);
                }
            }
        }
        assert_eq!(mask.get_pixel(15, 3)[0], 0);
        assert_eq!(mask.get_pixel(0, 0)[0], 255);
        assert_eq!(mask.get_pixel(31, 3)[0], 255);
        assert_eq!(mask.get_pixel(32, 3)[0], 0);
    }

    #[test]
    fn existing_text_mask_is_clipped_to_the_polygons_before_growing() {
        let mut text = GrayImage::new(40, 20);
        text.put_pixel(3, 3, Luma([255]));
        text.put_pixel(15, 3, Luma([255]));
        let mask = union_mask(
            40,
            20,
            Some(&text),
            &[Geometry::rectangle(2., 2., 4., 4.)],
            4,
        )
        .unwrap();
        assert_eq!(mask.get_pixel(7, 3)[0], 255);
        assert_eq!(mask.get_pixel(8, 3)[0], 0);
        assert_eq!(mask.get_pixel(15, 3)[0], 0);
    }

    #[test]
    fn missed_text_can_be_cleaned_when_the_rest_of_the_page_has_a_mask() {
        let mut text = GrayImage::new(40, 20);
        text.put_pixel(3, 3, Luma([255]));
        let mask = union_mask(
            40,
            20,
            Some(&text),
            &[Geometry::rectangle(24., 2., 4., 4.)],
            0,
        )
        .unwrap();
        assert_eq!(mask.get_pixel(25, 3)[0], 255);
        assert_eq!(mask.get_pixel(3, 3)[0], 0);
        assert_eq!(mask.get_pixel(23, 3)[0], 0);
    }
}
