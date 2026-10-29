use koharu_renderer::{TextStyle, TextStyles};
use koharu_scene::Point;
use koharu_scene::{
    BubbleRegion, EntityId, FlowsIn, Geometry, Inside, Region, RegionSpec, Snapshot,
};
use serde::Serialize;
use specta::Type;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Type)]
pub struct BubbleShapeInfo {
    pub bubble: Option<EntityId>,
    /// None without a bubble, or when its polygon is degenerate.
    pub shape: Option<BubbleShape>,
    pub measures: Option<ShapeMeasures>,
}

pub fn text_bubble_shape(snapshot: &Snapshot, layer: EntityId) -> anyhow::Result<BubbleShapeInfo> {
    let text = snapshot.text_layer(layer)?;
    let bubble = if let Some(relation) = snapshot.relation_from::<FlowsIn>(layer)? {
        Some(relation.value().target)
    } else if let Some(region) = text.content()?.source_region()? {
        let mut candidates = Vec::new();
        for relation in snapshot.relations_from_as::<Inside>(region.id()) {
            let id = relation.value().target;
            if snapshot
                .component::<Region>(id)?
                .is_some_and(|region| region.kind == BubbleRegion::kind())
            {
                let area = snapshot
                    .component::<Geometry>(id)?
                    .map(|geometry| signed_area(&geometry.points).abs())
                    .unwrap_or(f64::INFINITY);
                candidates.push((id, area));
            }
        }
        candidates
            .into_iter()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    } else {
        None
    };
    let bubble = if let Some(id) = bubble {
        snapshot
            .component::<Region>(id)?
            .filter(|region| region.kind == BubbleRegion::kind())
            .map(|_| id)
    } else {
        None
    };
    let measures = bubble
        .map(|id| snapshot.component::<Geometry>(id))
        .transpose()?
        .flatten()
        .and_then(|geometry| measure(&geometry.points));
    Ok(BubbleShapeInfo {
        bubble,
        shape: measures.map(|m| classify(&m, &ShapeThresholds::default())),
        measures,
    })
}

pub fn style_for_shape(styles: &TextStyles, shape: Option<BubbleShape>) -> Option<&TextStyle> {
    if !styles.shape_rules.enabled {
        return None;
    }
    let id = match shape {
        Some(BubbleShape::Rectangle) => &styles.shape_rules.rectangle,
        Some(BubbleShape::Round) => &styles.shape_rules.round,
        Some(BubbleShape::Spiky) => &styles.shape_rules.spiky,
        None => &styles.shape_rules.free,
    }
    .as_ref()?;
    styles
        .presets
        .iter()
        .find(|preset| &preset.id == id)
        .map(|preset| &preset.style)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BubbleShape {
    Rectangle,
    Round,
    Spiky,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Type)]
pub struct ShapeMeasures {
    pub rectangularity: f64,
    pub solidity: f64,
    pub spikes: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct ShapeThresholds {
    pub rect_min: f64,
    pub solidity_max: f64,
    pub spike_min: u32,
    pub defect_depth_fraction: f64,
}

impl Default for ShapeThresholds {
    fn default() -> Self {
        Self {
            rect_min: 0.90,
            solidity_max: 0.90,
            spike_min: 5,
            defect_depth_fraction: 0.08,
        }
    }
}

pub fn classify(measures: &ShapeMeasures, thresholds: &ShapeThresholds) -> BubbleShape {
    if measures.spikes >= thresholds.spike_min && measures.solidity < thresholds.solidity_max {
        BubbleShape::Spiky
    } else if measures.rectangularity >= thresholds.rect_min {
        BubbleShape::Rectangle
    } else {
        BubbleShape::Round
    }
}

/// Measure the scene polygon in f64, retaining subpixel edits and rotated boxes.
/// Hull indices keep each defect tied to its original stretch of the outline.
pub fn measure(polygon: &[Point]) -> Option<ShapeMeasures> {
    if polygon.len() < 3 || polygon.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return None;
    }
    let signed_area = signed_area(polygon);
    let area = signed_area.abs();
    if !area.is_finite() || area <= f64::EPSILON {
        return None;
    }
    let hull = convex_hull(polygon);
    if hull.len() < 3 {
        return None;
    }
    let hull_points = hull.iter().map(|&i| polygon[i]).collect::<Vec<_>>();
    let hull_area = self::signed_area(&hull_points).abs();
    let (rectangle_area, short_side) = min_rectangle(&hull_points)?;
    let minimum_depth = short_side * ShapeThresholds::default().defect_depth_fraction;
    let mut spikes = 0;
    for edge in 0..hull.len() {
        let from = hull[edge];
        let to = hull[(edge + 1) % hull.len()];
        let a = polygon[from];
        let b = polygon[to];
        let length = (b.x - a.x).hypot(b.y - a.y);
        let mut index = from;
        let mut depth: f64 = 0.0;
        loop {
            index = if signed_area > 0.0 {
                (index + 1) % polygon.len()
            } else {
                (index + polygon.len() - 1) % polygon.len()
            };
            if index == to {
                break;
            }
            depth = depth.max(cross(a, b, polygon[index]).abs() / length);
        }
        if depth > minimum_depth {
            spikes += 1;
        }
    }
    Some(ShapeMeasures {
        rectangularity: (area / rectangle_area).clamp(0.0, 1.0),
        solidity: (area / hull_area).clamp(0.0, 1.0),
        spikes,
    })
}

fn signed_area(points: &[Point]) -> f64 {
    let origin = points[0];
    // Translate before summing to avoid cancellation on large page coordinates.
    (1..points.len() - 1)
        .map(|i| cross(origin, points[i], points[i + 1]))
        .sum::<f64>()
        / 2.0
}

fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn convex_hull(points: &[Point]) -> Vec<usize> {
    let mut indices = (0..points.len()).collect::<Vec<_>>();
    indices.sort_by(|&a, &b| {
        points[a]
            .x
            .total_cmp(&points[b].x)
            .then(points[a].y.total_cmp(&points[b].y))
    });
    indices.dedup_by(|a, b| points[*a] == points[*b]);
    let mut lower: Vec<usize> = Vec::new();
    let mut upper: Vec<usize> = Vec::new();
    for &index in &indices {
        while lower.len() >= 2
            && cross(
                points[lower[lower.len() - 2]],
                points[lower[lower.len() - 1]],
                points[index],
            ) <= 0.0
        {
            lower.pop();
        }
        lower.push(index);
    }
    for &index in indices.iter().rev() {
        while upper.len() >= 2
            && cross(
                points[upper[upper.len() - 2]],
                points[upper[upper.len() - 1]],
                points[index],
            ) <= 0.0
        {
            upper.pop();
        }
        upper.push(index);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn min_rectangle(hull: &[Point]) -> Option<(f64, f64)> {
    let mut best = (f64::INFINITY, 0.0);
    for edge in 0..hull.len() {
        let origin = hull[edge];
        let next = hull[(edge + 1) % hull.len()];
        let dx = next.x - origin.x;
        let dy = next.y - origin.y;
        let length = dx.hypot(dy);
        if length <= f64::EPSILON {
            continue;
        }
        let (ux, uy) = (dx / length, dy / length);
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for p in hull {
            let x = (p.x - origin.x) * ux + (p.y - origin.y) * uy;
            let y = -(p.x - origin.x) * uy + (p.y - origin.y) * ux;
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        let width = max_x - min_x;
        let height = max_y - min_y;
        let area = width * height;
        if area < best.0 {
            best = (area, width.min(height));
        }
    }
    (best.0.is_finite() && best.0 > f64::EPSILON && best.1 > f64::EPSILON).then_some(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn radial(count: usize, radii: impl Fn(usize) -> (f64, f64)) -> Vec<Point> {
        (0..count)
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / count as f64;
                let (x, y) = radii(i);
                Point {
                    x: x * angle.cos(),
                    y: y * angle.sin(),
                }
            })
            .collect()
    }
    fn shape(points: &[Point]) -> BubbleShape {
        classify(&measure(points).unwrap(), &ShapeThresholds::default())
    }
    #[test]
    fn rectangles_survive_rotation_and_rounded_corners() {
        let rectangle = koharu_scene::Geometry::rectangle(0.0, 0.0, 200.0, 100.0).points;
        assert_eq!(shape(&rectangle), BubbleShape::Rectangle);
        let rotated = rectangle
            .iter()
            .map(|p| Point {
                x: 0.6 * p.x - 0.8 * p.y + 12.0,
                y: 0.8 * p.x + 0.6 * p.y - 30.0,
            })
            .collect::<Vec<_>>();
        assert_eq!(shape(&rotated), BubbleShape::Rectangle);
        let mut rounded = Vec::new();
        for (index, (x, y)) in [(190.0, 90.0), (10.0, 90.0), (10.0, 10.0), (190.0, 10.0)]
            .into_iter()
            .enumerate()
        {
            for step in 0..=8 {
                let angle = (index as f64 + step as f64 / 8.0) * std::f64::consts::FRAC_PI_2;
                rounded.push(Point {
                    x: x + 10.0 * angle.cos(),
                    y: y + 10.0 * angle.sin(),
                });
            }
        }
        assert_eq!(shape(&rounded), BubbleShape::Rectangle);
    }
    #[test]
    fn ellipse_and_one_tail_stay_round_in_both_windings() {
        let mut ellipse = radial(64, |_| (100.0, 60.0));
        let measures = measure(&ellipse).unwrap();
        assert!((measures.rectangularity - std::f64::consts::FRAC_PI_4).abs() < 0.01);
        assert_eq!(shape(&ellipse), BubbleShape::Round);
        ellipse[16] = Point { x: 30.0, y: 115.0 };
        assert_eq!(shape(&ellipse), BubbleShape::Round);
        assert!(measure(&ellipse).unwrap().spikes < 5);
        ellipse.reverse();
        assert_eq!(shape(&ellipse), BubbleShape::Round);
    }
    #[test]
    fn star_has_deep_defects_in_both_windings_and_at_any_scale() {
        let mut star = radial(24, |i| {
            let radius = if i % 2 == 0 { 100.0 } else { 60.0 };
            (radius, radius)
        });
        assert_eq!(shape(&star), BubbleShape::Spiky);
        assert_eq!(measure(&star).unwrap().spikes, 12);
        star.reverse();
        for p in &mut star {
            p.x = p.x * 0.001 + 5000.0;
            p.y = p.y * 0.001 + 5000.0;
        }
        assert_eq!(shape(&star), BubbleShape::Spiky);
    }
    #[test]
    fn existing_page_round_bubbles_keep_tails_without_becoming_spiky() {
        // White interior contours exercise real outlines without loading a detector model.
        let image = image::load_from_memory(include_bytes!(
            "../../../koharu-ml/benches/fixtures/object_detection/1.jpg"
        ))
        .unwrap()
        .to_rgb8();
        let mask = image::GrayImage::from_fn(image.width(), image.height(), |x, y| {
            image::Luma([if image.get_pixel(x, y).0.iter().all(|&c| c > 235) {
                255
            } else {
                0
            }])
        });
        let contours = imageproc::contours::find_contours_with_threshold::<i32>(&mask, 128);
        for (x, y) in [(300, 190), (370, 345), (115, 865), (650, 900)] {
            let polygon = contours
                .iter()
                .filter(|contour| contour.border_type == imageproc::contours::BorderType::Outer)
                .map(|contour| {
                    contour
                        .points
                        .iter()
                        .map(|p| Point {
                            x: p.x as f64,
                            y: p.y as f64,
                        })
                        .collect::<Vec<_>>()
                })
                .filter(|points| {
                    let min_x = points.iter().map(|p| p.x as i32).min().unwrap();
                    let max_x = points.iter().map(|p| p.x as i32).max().unwrap();
                    let min_y = points.iter().map(|p| p.y as i32).min().unwrap();
                    let max_y = points.iter().map(|p| p.y as i32).max().unwrap();
                    min_x < x
                        && x < max_x
                        && min_y < y
                        && y < max_y
                        && (44..180).contains(&(max_x - min_x))
                        && (70..260).contains(&(max_y - min_y))
                        && signed_area(points).abs() > 2000.0
                })
                .min_by(|a, b| signed_area(a).abs().total_cmp(&signed_area(b).abs()))
                .expect("fixture bubble contour");
            let measures = measure(&polygon).unwrap();
            println!("bubble ({x},{y}): {measures:?}");
            assert_eq!(
                classify(&measures, &ShapeThresholds::default()),
                BubbleShape::Round
            );
        }
    }

    #[test]
    fn degenerate_polygons_are_unmeasurable() {
        assert_eq!(measure(&[]), None);
        assert_eq!(measure(&[Point { x: 0.0, y: 0.0 }; 4]), None);
        assert_eq!(
            measure(
                &[Point {
                    x: f64::NAN,
                    y: 0.0
                }; 4]
            ),
            None
        );
    }
}
