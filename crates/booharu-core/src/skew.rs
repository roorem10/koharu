//! Skew: the FX window's lean of a text along each axis (S12-f), pure
//! maths over the scene's transforms.
//!
//! - What's drawn is the text's transform, which a skew is composed into
//!   about the text's pivot ([`pivot`]); the [`Skew`] record beside it holds
//!   the angles and that pivot, so the skew shows again as it was set and
//!   comes off exactly ([`reskewed`]). A record is trusted only while it
//!   still describes the transform, which a rotation or a move after the
//!   skew keeps it doing ([`trusted`]): one that doesn't, as after a shear
//!   from elsewhere, is never used to undo a skew (D1), and the skew shown
//!   is then the one the transform holds.
//! - Angles are degrees from −60 to 60. A positive `x` leans the text
//!   forward as italics do, its top to the right; a positive `y` lifts its
//!   right side. With `tx = tan x` and `ty = tan y`, the skew is `Sx·Sy`,
//!   `[[1 + tx·ty, −tx], [−ty, 1]]` in Koharu's rows, whose determinant is
//!   1, so a skew never changes a text's area ([`matrix`]). Never kurbo's
//!   `Affine::skew`, whose signs and order differ.
//! - A translated block's skew stays when the block is translated again,
//!   composed into placement's fresh transform about the new frame's pivot
//!   ([`carried`]; `styles::translated` writes it).

use koharu_scene::{Frame, Text, Transform};
use kurbo::{Affine, BezPath, Point, Rect, Shape as _};

use crate::attrs::Skew;

/// The steepest skew either way along each axis, in degrees.
pub const LIMIT: f64 = 60.0;

/// The least skew along an axis that counts as one, in degrees: less is a
/// rounding error of a turned box, which shows no skew, carries none, and
/// is no effect.
pub const LEAST: f64 = 0.05;

/// How far from square a trusted record leaves a transform: the cosine of
/// the angle between its columns once the record's skew is taken off.
const SQUARE: f64 = 1e-6;

/// Whether `degrees` is a skew Booharu writes: finite and from −60 to 60.
pub fn in_range(degrees: f64) -> bool {
    degrees.is_finite() && (-LIMIT..=LIMIT).contains(&degrees)
}

/// The skew of `x` and `y` degrees about the origin: `Sx·Sy`, the lean along
/// `x` after the lift along `y`, with a determinant of 1.
pub fn matrix(x: f64, y: f64) -> Affine {
    let (tx, ty) = (x.to_radians().tan(), y.to_radians().tan());
    Affine::new([1.0 + tx * ty, -ty, -tx, 1.0, 0.0, 0.0])
}

/// The point a text with `frame` leans about, in its own coordinates: its
/// box's centre, its path's bounding box's centre, or for point text its
/// anchor, the origin it starts at.
pub fn pivot(frame: &Frame) -> Point {
    match frame {
        Frame::Point => Point::ORIGIN,
        Frame::Box { bounds, .. } => Rect::from(*bounds).center(),
        Frame::Path { path, .. } => BezPath::from(path).bounding_box().center(),
    }
}

/// The skew of `x` and `y` degrees about `pivot`, which stays where it is.
fn about(pivot: Point, x: f64, y: f64) -> Affine {
    let to = Affine::translate(pivot.to_vec2());
    to * matrix(x, y) * to.inverse()
}

/// Whether `record` still describes `transform`: its linear part, with the
/// record's skew taken off, has square columns, within a millionth, and no
/// mirror, as after any move, rotation or even scaling of the skewed text.
/// A record out of range is never trusted.
pub fn trusted(transform: Transform, record: &Skew) -> bool {
    if !in_range(record.x) || !in_range(record.y) || !record.pivot.iter().all(|v| v.is_finite()) {
        return false;
    }
    let [a, b, c, d, _, _] = (linear(transform) * matrix(record.x, record.y).inverse()).as_coeffs();
    let norms = a.hypot(b) * c.hypot(d);
    let dot = a * c + b * d;
    a * d - b * c > 0.0 && norms > 0.0 && dot.abs() <= SQUARE * norms
}

/// `transform` without its translation.
fn linear(transform: Transform) -> Affine {
    let [a, b, c, d, _, _] = Affine::from(transform).as_coeffs();
    Affine::new([a, b, c, d, 0.0, 0.0])
}

/// The skew `text` shows, `(x, y)` in degrees: its record's while trusted
/// ([`trusted`]), else the lean its transform holds, as a rotation, a scale
/// and a shear make it (`R·diag(sx, sy)·[[1, k], [0, 1]]`), along `x` only;
/// a rounding error shows as none.
pub fn shown(text: &Text, record: Option<&Skew>) -> (f64, f64) {
    angles(text.transform, record)
}

/// [`shown`] for a text drawn with `transform`.
fn angles(transform: Transform, record: Option<&Skew>) -> (f64, f64) {
    match record.filter(|record| trusted(transform, record)) {
        Some(record) => (record.x, record.y),
        None => (decomposed(transform), 0.0),
    }
}

/// The lean along `x`, in degrees, that `transform` holds: `atan(−k)` for
/// its shear `k = (m00·m01 + m10·m11) / sx²`, as a forward lean has
/// `k = −tan x`; none for a transform that flattens the text.
fn decomposed(transform: Transform) -> f64 {
    let [a, b, c, d, _, _] = Affine::from(transform).as_coeffs();
    let squared = a * a + b * b;
    if squared <= f64::EPSILON {
        return 0.0;
    }
    let degrees = (-(a * c + b * d) / squared).atan().to_degrees();
    if degrees.abs() < 1e-9 { 0.0 } else { degrees }
}

/// Whether a text showing `(x, y)` is skewed: by at least [`LEAST`] along
/// either axis.
pub fn counts((x, y): (f64, f64)) -> bool {
    x.abs() >= LEAST || y.abs() >= LEAST
}

/// Whether `text`, with `record`, shows a skew ([`counts`]).
pub fn skewed(text: &Text, record: Option<&Skew>) -> bool {
    counts(shown(text, record))
}

/// `degrees` brought into −60 to 60, so a lean read from a shear made
/// elsewhere fits a record.
pub fn clamped(degrees: f64) -> f64 {
    if degrees.is_finite() {
        degrees.clamp(-LIMIT, LIMIT)
    } else {
        0.0
    }
}

/// `transform`, a text's, skewed `new` instead of the skew it shows: the
/// shown skew comes off about the trusted record's pivot, or about `pivot`
/// without a trusted record, and `new` goes on about `pivot`, the text's
/// pivot now ([`pivot`]), so a skew set and set back to 0 gives the
/// transform back, whatever was resized in between:
/// `T·C_old·S(old)⁻¹·C_old⁻¹·C_new·S(new)·C_new⁻¹`.
pub fn reskewed(
    transform: Transform,
    record: Option<&Skew>,
    new: (f64, f64),
    pivot: Point,
) -> Transform {
    let (old, from) = match record.filter(|record| trusted(transform, record)) {
        Some(record) => (
            (record.x, record.y),
            Point::new(record.pivot[0], record.pivot[1]),
        ),
        None => ((decomposed(transform), 0.0), pivot),
    };
    let undone = Affine::from(transform) * about(from, old.0, old.1).inverse();
    Transform::from(undone * about(pivot, new.0, new.1))
}

/// The skew `old`, a block's translation before it's translated again, with
/// `record`, carried onto its new placement, drawn with `transform`, placement's
/// fresh one, in `frame`: `transform` with the skew `old` shows composed in
/// about the new frame's pivot, and its record, at that pivot; none when
/// `old` shows no skew. An angle read from a shear made elsewhere is
/// brought into range.
pub fn carried(
    old: &Text,
    record: Option<&Skew>,
    transform: Transform,
    frame: &Frame,
) -> Option<(Transform, Skew)> {
    let shown = shown(old, record);
    if !counts(shown) {
        return None;
    }
    let (x, y) = (clamped(shown.0), clamped(shown.1));
    let pivot = pivot(frame);
    let skewed = Affine::from(transform) * about(pivot, x, y);
    Some((
        Transform::from(skewed),
        Skew {
            x,
            y,
            pivot: [pivot.x, pivot.y],
        },
    ))
}

#[cfg(test)]
mod tests {
    use koharu_scene::Bounds;
    use kurbo::Vec2;

    use super::*;

    /// A text in a box 40 by 120 at (10, 20), drawn with `transform`.
    fn text(transform: Affine) -> Text {
        Text {
            frame: boxed(10.0, 20.0, 40.0, 120.0),
            transform: Transform::from(transform),
            ..Text::default()
        }
    }

    fn boxed(x: f64, y: f64, width: f64, height: f64) -> Frame {
        Frame::Box {
            bounds: Bounds {
                x,
                y,
                width,
                height,
            },
            fit: Default::default(),
            anchor: Default::default(),
        }
    }

    /// The record of a skew set to `(x, y)` on `text` as it is.
    fn record(text: &Text, x: f64, y: f64) -> Skew {
        let pivot = pivot(&text.frame);
        Skew {
            x,
            y,
            pivot: [pivot.x, pivot.y],
        }
    }

    fn close(a: Transform, b: Transform) -> bool {
        let (a, b) = (Affine::from(a).as_coeffs(), Affine::from(b).as_coeffs());
        a.iter().zip(b).all(|(a, b)| (a - b).abs() <= 1e-9)
    }

    fn near(a: Point, b: Point) -> bool {
        (a - b).hypot() <= 1e-9
    }

    #[test]
    fn a_skew_keeps_the_area_and_inverts_even_at_45_both_ways() {
        for (x, y) in [
            (0.0, 0.0),
            (20.0, 0.0),
            (0.0, -30.0),
            (45.0, 45.0),
            (-60.0, 60.0),
        ] {
            let skew = matrix(x, y);
            assert!((skew.determinant() - 1.0).abs() <= 1e-12, "{x} {y}");
            let round = skew * skew.inverse();
            assert!(
                close(Transform::from(round), Transform::default()),
                "{x} {y}"
            );
        }
        assert!(in_range(60.0) && in_range(-60.0) && in_range(0.0));
        assert!(!in_range(60.5) && !in_range(f64::NAN) && !in_range(f64::INFINITY));
    }

    #[test]
    fn a_positive_x_leans_forward_and_a_positive_y_lifts_the_right_side() {
        // The page's y runs down: the top of the text is above its pivot.
        let lean = matrix(20.0, 0.0);
        let top = lean * Point::new(0.0, -10.0);
        assert!(top.x > 0.0 && (top.y + 10.0).abs() <= 1e-12, "{top:?}");
        let lift = matrix(0.0, 20.0);
        let right = lift * Point::new(10.0, 0.0);
        assert!(
            right.y < 0.0 && (right.x - 10.0).abs() <= 1e-12,
            "{right:?}"
        );
    }

    #[test]
    fn the_pivot_stays_put_on_the_page() {
        let turned = Affine::translate(Vec2::new(200.0, 50.0)) * Affine::rotate(0.3);
        let text = text(turned);
        let centre = pivot(&text.frame);
        assert_eq!(centre, Point::new(30.0, 80.0));
        let skewed = reskewed(text.transform, None, (20.0, -10.0), centre);
        assert!(near(
            Affine::from(skewed) * centre,
            Affine::from(text.transform) * centre
        ));
        // A path's pivot is its bounding box's centre, point text's its
        // anchor.
        let path = Frame::Path {
            path: koharu_scene::Path::try_from("M0 0 L40 0 L40 80 Z".to_owned()).expect("a path"),
            fit: Default::default(),
            anchor: Default::default(),
            inset: 1.0,
        };
        assert_eq!(pivot(&path), Point::new(20.0, 40.0));
        assert_eq!(pivot(&Frame::Point), Point::ORIGIN);
    }

    #[test]
    fn a_skew_set_and_set_back_gives_the_transform_back() {
        let placed = Affine::translate(Vec2::new(12.0, 7.0)) * Affine::rotate(-0.4);
        let mut text = text(placed);
        let centre = pivot(&text.frame);
        text.transform = reskewed(text.transform, None, (20.0, 15.0), centre);
        let set = record(&text, 20.0, 15.0);
        assert!(trusted(text.transform, &set));
        assert_eq!(shown(&text, Some(&set)), (20.0, 15.0));
        let back = reskewed(text.transform, Some(&set), (0.0, 0.0), centre);
        assert!(close(back, Transform::from(placed)));

        // Resized in between: the frame's centre moved, and the skew still
        // comes off about the pivot it went on about.
        text.frame = boxed(10.0, 20.0, 80.0, 60.0);
        let moved = pivot(&text.frame);
        assert_ne!(moved, centre);
        let back = reskewed(text.transform, Some(&set), (0.0, 0.0), moved);
        assert!(close(back, Transform::from(placed)));
    }

    #[test]
    fn a_rotation_after_the_skew_keeps_the_record_trusted() {
        let mut text = text(Affine::IDENTITY);
        let centre = pivot(&text.frame);
        text.transform = reskewed(text.transform, None, (25.0, -5.0), centre);
        let set = record(&text, 25.0, -5.0);
        // Turned and moved on the page, as the canvas turns a box, and
        // scaled evenly.
        let turn = Affine::translate(Vec2::new(40.0, -3.0))
            * Affine::rotate_about(0.7, Point::new(100.0, 100.0))
            * Affine::scale(1.5);
        text.transform = Transform::from(turn * Affine::from(text.transform));
        assert!(trusted(text.transform, &set));
        assert_eq!(shown(&text, Some(&set)), (25.0, -5.0));
    }

    #[test]
    fn a_shear_from_elsewhere_fails_the_check_and_shows_as_a_lean() {
        let mut text = text(Affine::IDENTITY);
        let centre = pivot(&text.frame);
        text.transform = reskewed(text.transform, None, (10.0, 0.0), centre);
        let set = record(&text, 10.0, 0.0);
        // Sheared again by something that kept the record.
        let shear = Affine::new([1.0, 0.0, -(30.0_f64.to_radians().tan()), 1.0, 0.0, 0.0]);
        let sheared = Transform::from(Affine::from(text.transform) * shear);
        assert!(!trusted(sheared, &set));
        text.transform = sheared;
        let (x, y) = shown(&text, Some(&set));
        // tan 10° + tan 30°, read back as one lean.
        let expected = (10.0_f64.to_radians().tan() + 30.0_f64.to_radians().tan())
            .atan()
            .to_degrees();
        assert!((x - expected).abs() <= 1e-9 && y == 0.0, "{x} {y}");
        // A record kept by a text never skewed isn't trusted either, and
        // nothing shows.
        let plain = self::text(Affine::rotate(0.2));
        assert!(!trusted(plain.transform, &set));
        assert_eq!(shown(&plain, Some(&set)), (0.0, 0.0));
        assert!(!skewed(&plain, Some(&set)));
        // Undoing what shows takes the whole lean off, about the pivot,
        // which stays put.
        let back = reskewed(text.transform, Some(&set), (0.0, 0.0), centre);
        let [a, b, c, d, _, _] = Affine::from(back).as_coeffs();
        assert!(close(
            Transform::from(Affine::new([a, b, c, d, 0.0, 0.0])),
            Transform::default()
        ));
        assert!(near(
            Affine::from(back) * centre,
            Affine::from(text.transform) * centre
        ));
    }

    #[test]
    fn changing_one_axis_keeps_the_other() {
        let mut text = text(Affine::rotate(0.25));
        let centre = pivot(&text.frame);
        text.transform = reskewed(text.transform, None, (20.0, 15.0), centre);
        let set = record(&text, 20.0, 15.0);
        let (_, y) = shown(&text, Some(&set));
        text.transform = reskewed(text.transform, Some(&set), (-10.0, y), centre);
        let set = record(&text, -10.0, y);
        assert!(trusted(text.transform, &set));
        assert_eq!(shown(&text, Some(&set)), (-10.0, 15.0));
        let expected = Affine::rotate(0.25) * about(centre, -10.0, 15.0);
        assert!(close(text.transform, Transform::from(expected)));
    }

    #[test]
    fn a_skew_carries_onto_a_new_placement_about_its_new_pivot() {
        let mut old = text(Affine::rotate(0.1));
        let centre = pivot(&old.frame);
        old.transform = reskewed(old.transform, None, (20.0, 0.0), centre);
        let set = record(&old, 20.0, 0.0);
        // Placed again, turned another way in another frame.
        let fresh = Affine::rotate_about(-0.3, Point::new(50.0, 50.0));
        let frame = boxed(0.0, 0.0, 60.0, 100.0);
        let (transform, carried) =
            carried(&old, Some(&set), Transform::from(fresh), &frame).expect("a skew");
        let new_pivot = Point::new(30.0, 50.0);
        assert_eq!(carried.pivot, [30.0, 50.0]);
        assert_eq!((carried.x, carried.y), (20.0, 0.0));
        assert!(close(
            transform,
            Transform::from(fresh * about(new_pivot, 20.0, 0.0))
        ));
        assert!(near(Affine::from(transform) * new_pivot, fresh * new_pivot));
        assert!(trusted(transform, &carried));
        // Nothing to carry from a text showing none.
        let plain = text(Affine::rotate(0.1));
        assert_eq!(carried_of(&plain), None);
    }

    fn carried_of(old: &Text) -> Option<(Transform, Skew)> {
        carried(old, None, Transform::default(), &old.frame)
    }
}
