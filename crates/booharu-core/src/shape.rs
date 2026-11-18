//! Bubble shapes: a balloon's outline classified as round, rectangular, or
//! spiky, as Booharu 0.0.4's `bubble_shape.rs` measured one, ported over
//! kurbo paths. Its curves are flattened and the result simplified by
//! Douglas–Peucker at 0.1% of its length, as Koharu's detection simplifies
//! an outline it finds, and three measures decide: its area over that of
//! the smallest rotated rectangle around it (rectangularity), over that of
//! its convex hull (solidity), and its convexity defects deeper than 8% of
//! that rectangle's short side (spikes). It's spiky with 5 spikes and a
//! solidity under 0.90, else rectangular with a rectangularity of 0.90, else
//! round.
//!
//! A block's balloon is the balloon holding it, else the smallest balloon on
//! its page whose outline holds its source's centre, as a box drawn by hand
//! may sit outside one; none for a block in neither. The shape is worked out
//! when asked and never stored, so an outline edited by hand classifies as
//! it is.

use koharu_scene::{Content, Layer, LayerId, Page};
use kurbo::{BezPath, PathEl, Point, Shape as _};
use serde::{Deserialize, Serialize};

/// The shape of a balloon's outline.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum BubbleShape {
    /// A rounded balloon, speech's.
    Round,
    /// A rectangular box, narration's.
    Rectangle,
    /// A balloon with spikes, a shout's.
    Spiky,
}

/// What decided a balloon's shape.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ShapeMeasures {
    /// Its area over that of the smallest rotated rectangle around it, from
    /// 0 to 1.
    pub rectangularity: f64,
    /// Its area over that of its convex hull, from 0 to 1.
    pub solidity: f64,
    /// How many dips in its outline go deeper than 8% of that rectangle's
    /// short side.
    pub spikes: u32,
}

/// Where [`classify`] draws its lines, Booharu 0.0.4's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeThresholds {
    /// The least rectangularity of a rectangle.
    pub rect_min: f64,
    /// A spiky balloon's solidity is under this.
    pub solidity_max: f64,
    /// The fewest spikes of a spiky balloon.
    pub spike_min: u32,
    /// How deep a dip goes to count as a spike, as a share of the smallest
    /// rotated rectangle's short side.
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

/// A block's balloon's shape, and what decided it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct BlockShape {
    pub shape: BubbleShape,
    pub measures: ShapeMeasures,
}

/// The shape `measures` make under `thresholds`.
pub fn classify(measures: &ShapeMeasures, thresholds: &ShapeThresholds) -> BubbleShape {
    if measures.spikes >= thresholds.spike_min && measures.solidity < thresholds.solidity_max {
        BubbleShape::Spiky
    } else if measures.rectangularity >= thresholds.rect_min {
        BubbleShape::Rectangle
    } else {
        BubbleShape::Round
    }
}

/// How finely an outline's curves are flattened, in page pixels. Simplifying
/// then drops what lies within 0.1% of the outline's length, which is coarser
/// than this for an outline longer than 250 pixels, a balloon about 80
/// across; on a smaller one this is the coarser of the two, though still far
/// too fine to move its measures, as a dip counts only past 8% of the short
/// side of the smallest rectangle around it.
const FLATNESS: f64 = 0.25;

/// What decides the shape of `outline`, a closed path in page pixels:
/// measured on its largest part, flattened and simplified. None when it
/// encloses nothing, or has a point that isn't finite.
pub fn measure(outline: &BezPath) -> Option<ShapeMeasures> {
    let largest = polygons(outline)
        .into_iter()
        .filter(|polygon| polygon.len() >= 3)
        .max_by(|a, b| signed_area(a).abs().total_cmp(&signed_area(b).abs()))?;
    measure_polygon(&simplified(&largest))
}

/// The closed polygons `outline`'s parts flatten to, without a last point
/// that repeats the first.
fn polygons(outline: &BezPath) -> Vec<Vec<Point>> {
    let mut polygons = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    let mut start = None;
    kurbo::flatten(outline.iter(), FLATNESS, |element| match element {
        PathEl::MoveTo(point) => {
            polygons.push(std::mem::take(&mut current));
            current.push(point);
            start = Some(point);
        }
        PathEl::LineTo(point) => {
            // A part going on after it closed starts where it started.
            if current.is_empty()
                && let Some(start) = start
            {
                current.push(start);
            }
            current.push(point);
        }
        PathEl::ClosePath => polygons.push(std::mem::take(&mut current)),
        // Flattening leaves only lines.
        PathEl::QuadTo(..) | PathEl::CurveTo(..) => {}
    });
    polygons.push(current);
    for polygon in &mut polygons {
        if polygon.len() > 1 && polygon.first() == polygon.last() {
            polygon.pop();
        }
    }
    polygons
}

/// `polygon`, closed, simplified by Douglas–Peucker at 0.1% of its length,
/// as Koharu's detection simplifies an outline it finds. Unlike imageproc's
/// `approximate_polygon_dp` on a closed curve, which it uses on a pixel
/// contour and which drops the contour's last point, every corner stays: a
/// polygon here may be simplified already, so its last point is one.
fn simplified(polygon: &[Point]) -> Vec<Point> {
    let length: f64 = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .map(|(a, b)| a.distance(*b))
        .sum();
    let epsilon = (length * 0.001).max(f64::EPSILON);
    let mut ring = polygon.to_vec();
    ring.push(polygon[0]);
    let mut kept = douglas_peucker(&ring, epsilon);
    kept.pop();
    kept
}

/// The points of `curve` Douglas–Peucker keeps at `epsilon`, its ends
/// included: from a closed curve's first point, which is also its last, the
/// farthest point splits it first.
fn douglas_peucker(curve: &[Point], epsilon: f64) -> Vec<Point> {
    let (first, last) = (curve[0], curve[curve.len() - 1]);
    let chord = last - first;
    let length = chord.hypot();
    let distance = |point: Point| match length > 0.0 {
        true => chord.cross(point - first).abs() / length,
        false => point.distance(first),
    };
    let (index, farthest) =
        curve
            .iter()
            .enumerate()
            .skip(1)
            .fold((0, 0.0), |(index, farthest), (at, &point)| {
                let away = distance(point);
                match away > farthest {
                    true => (at, away),
                    false => (index, farthest),
                }
            });
    if farthest <= epsilon {
        return vec![first, last];
    }
    let mut kept = douglas_peucker(&curve[..=index], epsilon);
    kept.pop();
    kept.extend(douglas_peucker(&curve[index..], epsilon));
    kept
}

/// Booharu 0.0.4's `measure`: the measures of `polygon`, a closed polygon in
/// page pixels, in f64, so subpixel edits and rotated boxes keep theirs.
/// Each dip is measured along its own stretch of the outline between two of
/// the hull's corners, so it stays tied to the place it is.
fn measure_polygon(polygon: &[Point]) -> Option<ShapeMeasures> {
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
    // Translated to the first point before summing, so large page
    // coordinates don't cancel out.
    (1..points.len() - 1)
        .map(|i| cross(origin, points[i], points[i + 1]))
        .sum::<f64>()
        / 2.0
}

fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// The indexes of `points` on their convex hull, by Andrew's monotone chain.
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

/// The area and the short side of the smallest rectangle around `hull`,
/// turned to lie along one of its edges.
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

/// The layer `id` among `page`'s layers and inside them, and the layer
/// holding it, if one does.
pub(crate) fn find(page: &Page, id: LayerId) -> Option<(&Layer, Option<&Layer>)> {
    fn search<'p>(
        layers: &'p [Layer],
        holder: Option<&'p Layer>,
        id: LayerId,
    ) -> Option<(&'p Layer, Option<&'p Layer>)> {
        layers.iter().find_map(|layer| match layer.id == id {
            true => Some((layer, holder)),
            false => search(&layer.children, Some(layer), id),
        })
    }
    search(&page.layers, None, id)
}

/// The balloon the block `block` on `page` is in: the one holding it, else
/// the smallest whose outline holds its source's centre; none for a layer
/// that isn't a block, or a block in neither.
pub fn balloon(page: &Page, block: LayerId) -> Option<&Layer> {
    let (layer, holder) = find(page, block)?;
    let Content::Block(found) = &layer.content else {
        return None;
    };
    if let Some(holder) = holder.filter(|holder| matches!(holder.content, Content::Balloon(_))) {
        return Some(holder);
    }
    let centre = found.source.bounds()?.center();
    let mut balloons = Vec::new();
    every(&page.layers, &mut |layer| {
        if let Content::Balloon(balloon) = &layer.content {
            let outline = BezPath::from(&balloon.outline);
            if outline.contains(centre) {
                balloons.push((layer, outline.area().abs()));
            }
        }
    });
    balloons
        .into_iter()
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(layer, _)| layer)
}

/// Calls `visit` on each of `layers` and everything inside them, depth first.
fn every<'p>(layers: &'p [Layer], visit: &mut impl FnMut(&'p Layer)) {
    for layer in layers {
        visit(layer);
        every(&layer.children, visit);
    }
}

/// The shape of the balloon the block `block` on `page` is in, with what
/// decided it; none without a balloon, or with an outline that encloses
/// nothing.
pub fn block_shape(page: &Page, block: LayerId) -> Option<BlockShape> {
    let Content::Balloon(found) = &balloon(page, block)?.content else {
        return None;
    };
    let measures = measure(&BezPath::from(&found.outline))?;
    Some(BlockShape {
        shape: classify(&measures, &ShapeThresholds::default()),
        measures,
    })
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Created, Edit, Op, Session};
    use kurbo::{Ellipse, Rect};
    use serde_json::{Value, json};

    use super::*;

    fn radial(count: usize, radii: impl Fn(usize) -> (f64, f64)) -> Vec<Point> {
        (0..count)
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / count as f64;
                let (x, y) = radii(i);
                Point::new(x * angle.cos(), y * angle.sin())
            })
            .collect()
    }

    fn rectangle(x: f64, y: f64, width: f64, height: f64) -> Vec<Point> {
        vec![
            Point::new(x, y),
            Point::new(x + width, y),
            Point::new(x + width, y + height),
            Point::new(x, y + height),
        ]
    }

    fn shape(points: &[Point]) -> BubbleShape {
        classify(
            &measure_polygon(points).expect("measures"),
            &ShapeThresholds::default(),
        )
    }

    /// The closed path through `points`.
    fn closed(points: &[Point]) -> BezPath {
        let mut path = BezPath::new();
        path.move_to(points[0]);
        for point in &points[1..] {
            path.line_to(*point);
        }
        path.close_path();
        path
    }

    // Booharu 0.0.4's tests, on the polygons it measured.

    #[test]
    fn rectangles_survive_rotation_and_rounded_corners() {
        let rectangle = rectangle(0.0, 0.0, 200.0, 100.0);
        assert_eq!(shape(&rectangle), BubbleShape::Rectangle);
        let rotated = rectangle
            .iter()
            .map(|p| Point::new(0.6 * p.x - 0.8 * p.y + 12.0, 0.8 * p.x + 0.6 * p.y - 30.0))
            .collect::<Vec<_>>();
        assert_eq!(shape(&rotated), BubbleShape::Rectangle);
        let mut rounded = Vec::new();
        for (index, (x, y)) in [(190.0, 90.0), (10.0, 90.0), (10.0, 10.0), (190.0, 10.0)]
            .into_iter()
            .enumerate()
        {
            for step in 0..=8 {
                let angle = (index as f64 + step as f64 / 8.0) * std::f64::consts::FRAC_PI_2;
                rounded.push(Point::new(x + 10.0 * angle.cos(), y + 10.0 * angle.sin()));
            }
        }
        assert_eq!(shape(&rounded), BubbleShape::Rectangle);
    }

    #[test]
    fn ellipse_and_one_tail_stay_round_in_both_windings() {
        let mut ellipse = radial(64, |_| (100.0, 60.0));
        let measures = measure_polygon(&ellipse).expect("measures");
        assert!((measures.rectangularity - std::f64::consts::FRAC_PI_4).abs() < 0.01);
        assert_eq!(shape(&ellipse), BubbleShape::Round);
        ellipse[16] = Point::new(30.0, 115.0);
        assert_eq!(shape(&ellipse), BubbleShape::Round);
        assert!(measure_polygon(&ellipse).expect("measures").spikes < 5);
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
        assert_eq!(measure_polygon(&star).expect("measures").spikes, 12);
        star.reverse();
        for p in &mut star {
            p.x = p.x * 0.001 + 5000.0;
            p.y = p.y * 0.001 + 5000.0;
        }
        assert_eq!(shape(&star), BubbleShape::Spiky);
    }

    #[test]
    fn existing_page_round_bubbles_keep_tails_without_becoming_spiky() {
        use imageproc::contours::{BorderType, find_contours_with_threshold};

        // White interior contours exercise real outlines without loading a
        // detector model.
        let image = image::load_from_memory(include_bytes!(
            "../../koharu-ml/benches/fixtures/object_detection/1.jpg"
        ))
        .expect("the fixture page")
        .to_rgb8();
        let mask = image::GrayImage::from_fn(image.width(), image.height(), |x, y| {
            let white = image.get_pixel(x, y).0.iter().all(|&c| c > 235);
            image::Luma([if white { 255 } else { 0 }])
        });
        let contours = find_contours_with_threshold::<i32>(&mask, 128);
        for (x, y) in [
            (300.0, 190.0),
            (370.0, 345.0),
            (115.0, 865.0),
            (650.0, 900.0),
        ] {
            let outline = contours
                .iter()
                .filter(|contour| contour.border_type == BorderType::Outer)
                .map(|contour| {
                    let point = |p: &imageproc::point::Point<i32>| {
                        Point::new(f64::from(p.x), f64::from(p.y))
                    };
                    contour.points.iter().map(point).collect::<Vec<_>>()
                })
                .filter(|points| {
                    let bounds = closed(points).bounding_box();
                    bounds.x0 < x
                        && x < bounds.x1
                        && bounds.y0 < y
                        && y < bounds.y1
                        && (44.0..180.0).contains(&bounds.width())
                        && (70.0..260.0).contains(&bounds.height())
                        && signed_area(points).abs() > 2000.0
                })
                .min_by(|a, b| signed_area(a).abs().total_cmp(&signed_area(b).abs()))
                .expect("the fixture's bubble");
            // As 0.0.4 measured the traced outline, then as an outline in
            // the scene is measured, simplified first, which drops most of
            // a pixel outline's stair-steps.
            assert_eq!(shape(&outline), BubbleShape::Round, "({x}, {y})");
            assert!(simplified(&outline).len() < outline.len() / 2, "({x}, {y})");
            let measures = measure(&closed(&outline)).expect("measures");
            assert_eq!(
                classify(&measures, &ShapeThresholds::default()),
                BubbleShape::Round,
                "({x}, {y}): {measures:?}"
            );
        }
    }

    #[test]
    fn degenerate_polygons_are_unmeasurable() {
        assert_eq!(measure_polygon(&[]), None);
        assert_eq!(measure_polygon(&[Point::ZERO; 4]), None);
        assert_eq!(measure_polygon(&[Point::new(f64::NAN, 0.0); 4]), None);
        assert_eq!(measure(&BezPath::new()), None);
        assert_eq!(
            measure(&closed(&[Point::ZERO, Point::new(10.0, 0.0)])),
            None
        );
    }

    // The same, over paths as the scene holds outlines.

    #[test]
    fn a_path_is_flattened_and_simplified_before_it_is_measured() {
        let classified = |path: &BezPath| {
            classify(
                &measure(path).expect("measures"),
                &ShapeThresholds::default(),
            )
        };
        // Every corner of a polygon stays.
        assert_eq!(
            classified(&closed(&rectangle(0.0, 0.0, 200.0, 100.0))),
            BubbleShape::Rectangle
        );
        // Curves: an ellipse drawn with them is round, a rounded rectangle
        // a rectangle.
        let ellipse = Ellipse::new((300.0, 200.0), (100.0, 60.0), 0.0).to_path(0.1);
        assert!(
            ellipse
                .elements()
                .iter()
                .any(|el| matches!(el, PathEl::CurveTo(..)))
        );
        assert_eq!(classified(&ellipse), BubbleShape::Round);
        let rounded = Rect::new(0.0, 0.0, 200.0, 100.0)
            .to_rounded_rect(10.0)
            .to_path(0.1);
        assert_eq!(classified(&rounded), BubbleShape::Rectangle);
        let star = radial(24, |i| {
            let radius = if i % 2 == 0 { 100.0 } else { 60.0 };
            (radius, radius)
        });
        assert_eq!(classified(&closed(&star)), BubbleShape::Spiky);
        // A last point repeating the first changes nothing, and the largest
        // part is the one measured.
        let mut repeated = rectangle(0.0, 0.0, 200.0, 100.0);
        repeated.push(repeated[0]);
        let mut two = closed(&repeated);
        two.extend(closed(&star).elements().iter().map(|el| match *el {
            PathEl::MoveTo(p) => PathEl::MoveTo(Point::new(p.x * 0.1, p.y * 0.1)),
            PathEl::LineTo(p) => PathEl::LineTo(Point::new(p.x * 0.1, p.y * 0.1)),
            other => other,
        }));
        assert_eq!(classified(&two), BubbleShape::Rectangle);
    }

    #[test]
    fn simplifying_drops_points_on_a_straight_stretch_only() {
        let mut dense = Vec::new();
        for x in 0..=200 {
            dense.push(Point::new(f64::from(x), 0.0));
        }
        dense.extend([Point::new(200.0, 100.0), Point::new(0.0, 100.0)]);
        let kept = simplified(&dense);
        assert_eq!(kept.len(), 4, "{kept:?}");
        for corner in rectangle(0.0, 0.0, 200.0, 100.0) {
            assert!(kept.contains(&corner), "{corner:?} in {kept:?}");
        }
    }

    /// A page of 400 by 400 holding `layers`, as add_layer ops' JSON, and the
    /// ids of the layers labelled.
    fn page(layers: &[Value]) -> (Page, impl Fn(&str) -> LayerId + use<>) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let mut ops = vec![
            json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}),
        ];
        ops.extend(layers.iter().cloned());
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(|op| serde_json::from_value::<Op>(op).expect("an op"))
                    .collect(),
            })
            .expect("a page");
        let ids: Vec<(String, LayerId)> = change
            .created
            .iter()
            .filter_map(|created| match created {
                Created::Layer {
                    id,
                    label: Some(label),
                } => Some((label.clone(), *id)),
                _ => None,
            })
            .collect();
        let project = session.project();
        let page = project
            .page(change.pages.added[0])
            .expect("the page")
            .clone();
        let id = move |label: &str| {
            ids.iter()
                .find_map(|(found, id)| (found == label).then_some(*id))
                .expect("a layer")
        };
        (page, id)
    }

    fn balloon_layer(label: &str, outline: &BezPath) -> Value {
        json!({"op": "add_layer", "id": format!("${label}"), "page": "$p", "index": 0,
            "layer": {"kind": "balloon", "outline": outline.to_svg()}})
    }

    fn block_layer(label: &str, parent: Option<&str>, x: f64, y: f64) -> Value {
        json!({"op": "add_layer", "id": format!("${label}"), "page": "$p",
        "parent": parent.map(|parent| format!("${parent}")), "index": 0,
        "layer": {"kind": "block", "source": {
            "frame": {"box": {"bounds": {"x": x, "y": y, "width": 20.0, "height": 20.0}}},
        }}})
    }

    #[test]
    fn a_blocks_balloon_holds_it_or_its_centre() {
        let big = closed(&rectangle(0.0, 0.0, 300.0, 300.0));
        let small = Ellipse::new((100.0, 100.0), (50.0, 30.0), 0.0).to_path(0.1);
        let (page, id) = page(&[
            balloon_layer("big", &big),
            balloon_layer("small", &small),
            balloon_layer("holder", &closed(&rectangle(320.0, 0.0, 60.0, 60.0))),
            // Held by a balloon its centre is outside of.
            block_layer("held", Some("holder"), 10.0, 350.0),
            // In both rectangles: the smaller, round one is its balloon.
            block_layer("inside", None, 90.0, 90.0),
            // In the big one only.
            block_layer("corner", None, 250.0, 250.0),
            // In none.
            block_layer("free", None, 350.0, 350.0),
        ]);
        assert_eq!(balloon(&page, id("held")).map(|b| b.id), Some(id("holder")));
        assert_eq!(
            balloon(&page, id("inside")).map(|b| b.id),
            Some(id("small"))
        );
        assert_eq!(balloon(&page, id("corner")).map(|b| b.id), Some(id("big")));
        assert_eq!(balloon(&page, id("free")), None);
        // Only a block has a balloon.
        assert_eq!(balloon(&page, id("big")), None);

        let shape = |label: &str| block_shape(&page, id(label)).map(|found| found.shape);
        assert_eq!(shape("inside"), Some(BubbleShape::Round));
        assert_eq!(shape("corner"), Some(BubbleShape::Rectangle));
        assert_eq!(shape("held"), Some(BubbleShape::Rectangle));
        assert_eq!(shape("free"), None);
        let measures = block_shape(&page, id("corner")).expect("a shape").measures;
        assert_eq!(
            (measures.rectangularity, measures.solidity, measures.spikes),
            (1.0, 1.0, 0)
        );
    }
}
