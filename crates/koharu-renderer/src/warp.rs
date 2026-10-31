use anyhow::{Result, ensure};
use koharu_scene::TextWarp;
use vello::kurbo::{Affine, BezPath, PathEl, Point, Shape, flatten};

use crate::bubble::GeometryFrame;

pub(crate) fn text_placement(frame: GeometryFrame) -> Affine {
    let bounds = frame.bounds;
    Affine::translate((
        f64::from(bounds.x + bounds.width * 0.5),
        f64::from(bounds.y + bounds.height * 0.5),
    )) * Affine::rotate(f64::from(frame.angle_degrees).to_radians())
        * Affine::translate((
            -f64::from(bounds.width * 0.5),
            -f64::from(bounds.height * 0.5),
        ))
}

pub(crate) fn skew(warp: Option<&TextWarp>, width: f32, height: f32) -> Affine {
    let Some(TextWarp::Skew {
        x_degrees,
        y_degrees,
    }) = warp
    else {
        return Affine::IDENTITY;
    };
    let center = (f64::from(width) * 0.5, f64::from(height) * 0.5);
    Affine::translate(center)
        * Affine::new([
            1.0,
            f64::from(*y_degrees).to_radians().tan(),
            f64::from(*x_degrees).to_radians().tan(),
            1.0,
            0.0,
            0.0,
        ])
        * Affine::translate((-center.0, -center.1))
}

pub(crate) struct Perspective {
    x: [f64; 3],
    y: [f64; 3],
    denominator: [f64; 2],
    width: f64,
    height: f64,
}

impl Perspective {
    pub(crate) fn new(corners: [[f32; 2]; 4], width: f32, height: f32) -> Result<Self> {
        TextWarp::Perspective { corners }.validate()?;
        ensure!(
            width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0,
            "invalid text warp frame"
        );
        let [p, q, r, s] = corners.map(|point| point.map(f64::from));
        let dx1 = q[0] - r[0];
        let dx2 = s[0] - r[0];
        let dx3 = p[0] - q[0] + r[0] - s[0];
        let dy1 = q[1] - r[1];
        let dy2 = s[1] - r[1];
        let dy3 = p[1] - q[1] + r[1] - s[1];
        let determinant = dx1 * dy2 - dx2 * dy1;
        ensure!(determinant != 0.0, "degenerate text perspective");
        let g = (dx3 * dy2 - dx2 * dy3) / determinant;
        let h = (dx1 * dy3 - dx3 * dy1) / determinant;
        Ok(Self {
            x: [q[0] - p[0] + g * q[0], s[0] - p[0] + h * s[0], p[0]],
            y: [q[1] - p[1] + g * q[1], s[1] - p[1] + h * s[1], p[1]],
            denominator: [g, h],
            width: f64::from(width),
            height: f64::from(height),
        })
    }

    fn divisor(&self, point: Point) -> f64 {
        1.0 + self.denominator[0] * point.x / self.width
            + self.denominator[1] * point.y / self.height
    }

    fn map(&self, point: Point) -> Point {
        let u = point.x / self.width;
        let v = point.y / self.height;
        let divisor = self.divisor(point);
        Point::new(
            self.width * (self.x[0] * u + self.x[1] * v + self.x[2]) / divisor,
            self.height * (self.y[0] * u + self.y[1] * v + self.y[2]) / divisor,
        )
    }

    pub(crate) fn path(&self, path: &BezPath) -> Result<BezPath> {
        if path.is_empty() {
            return Ok(BezPath::new());
        }
        let bounds = path.bounding_box();
        let corners = [
            Point::new(bounds.x0, bounds.y0),
            Point::new(bounds.x1, bounds.y0),
            Point::new(bounds.x1, bounds.y1),
            Point::new(bounds.x0, bounds.y1),
        ];
        // Overflow and wide outlines can extend past the frame. Refuse a projective horizon
        // through that drawing rather than producing infinite coordinates or an enormous crop.
        let minimum = corners
            .into_iter()
            .map(|point| self.divisor(point))
            .fold(f64::INFINITY, f64::min);
        ensure!(
            minimum > 1e-8,
            "text extends across the perspective horizon"
        );
        let [g, h] = self.denominator;
        let mut gain: f64 = 1.0;
        for point in corners {
            let u = point.x / self.width;
            let v = point.y / self.height;
            let [a, b, c] = self.x;
            let [d, e, f] = self.y;
            let xx = (a - c * g + (a * h - b * g) * v).abs();
            let xy = (b - c * h + (b * g - a * h) * u).abs() * self.width / self.height;
            let yx = (d - f * g + (d * h - e * g) * v).abs() * self.height / self.width;
            let yy = (e - f * h + (e * g - d * h) * u).abs();
            gain = gain.max((xx + xy + yx + yy) / minimum.powi(2));
        }
        let mut result = BezPath::new();
        flatten(
            path.elements().iter().copied(),
            (0.02 / gain).max(1e-6),
            |element| {
                result.push(match element {
                    PathEl::MoveTo(p) => PathEl::MoveTo(self.map(p)),
                    PathEl::LineTo(p) => PathEl::LineTo(self.map(p)),
                    PathEl::ClosePath => PathEl::ClosePath,
                    _ => unreachable!("flatten emits only straight segments"),
                });
            },
        );
        ensure!(
            result.elements().iter().all(|element| match element {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => p.x.is_finite() && p.y.is_finite(),
                _ => true,
            }),
            "text perspective produced nonfinite coordinates"
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vello::kurbo::Rect;

    #[test]
    fn skew_is_centered_and_moves_the_top_edge_relative_to_the_bottom() {
        let affine = skew(
            Some(&TextWarp::Skew {
                x_degrees: 30.0,
                y_degrees: 0.0,
            }),
            200.0,
            40.0,
        );
        assert_eq!(affine * Point::new(100.0, 20.0), Point::new(100.0, 20.0));
        let top = affine * Point::new(0.0, 0.0);
        let bottom = affine * Point::new(0.0, 40.0);
        assert!((bottom.x - top.x - 40.0 * 30.0_f64.to_radians().tan()).abs() < 1e-9);
        assert_eq!(skew(None, 200.0, 40.0), Affine::IDENTITY);
    }

    #[test]
    fn projective_paths_map_corners_and_reject_a_horizon_in_overflow() {
        let corners = [[0.25, 0.0], [0.75, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let perspective = Perspective::new(corners, 200.0, 100.0).unwrap();
        for (input, target) in [[0.0, 0.0], [200.0, 0.0], [200.0, 100.0], [0.0, 100.0]]
            .into_iter()
            .zip(corners)
        {
            let output = perspective.map(Point::new(input[0], input[1]));
            assert!((output.x - f64::from(target[0]) * 200.0).abs() < 1e-8);
            assert!((output.y - f64::from(target[1]) * 100.0).abs() < 1e-8);
        }
        assert!(
            perspective
                .path(&Rect::new(0.0, 0.0, 200.0, 250.0).to_path(0.1))
                .is_err()
        );
    }
}
