use std::collections::BTreeMap;

use revision::revisioned;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    Error, Result,
    component::{Component, ValidationContext},
    id::validate_namespaced,
};

use super::Origin;

#[revisioned(revision = 1)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RasterLayerKind {
    Cleanup,
    Paint,
}

/// A full-page transparent pixel layer composited above the page source.
#[revisioned(revision = 1)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct RasterLayer {
    pub origin: Origin,
    pub name: String,
    pub kind: RasterLayerKind,
}

impl Component for RasterLayer {
    const KIND: &'static str = "dev.koharu.layer.raster";

    fn validate(&self, _context: &ValidationContext<'_>) -> Result<()> {
        self.origin.validate()?;
        if self.name.is_empty() || self.name.len() > 4096 || self.name.contains('\0') {
            return Err(Error::invalid("raster layer name is invalid"));
        }
        Ok(())
    }

    fn origin(&self) -> Option<&Origin> {
        Some(&self.origin)
    }

    fn set_origin(&mut self, origin: Origin) -> bool {
        self.origin = origin;
        true
    }
}

#[revisioned(revision = 1)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TextLayoutKind {
    Point,
    Paragraph,
}

#[revisioned(revision = 3)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct TextLayout {
    pub origin: Origin,
    pub kind: TextLayoutKind,
    /// Overrides the axes inferred from geometry when a frame is transformed.
    #[revision(start = 2)]
    pub angle_degrees: Option<f32>,
    #[revision(start = 3)]
    pub warp: Option<TextWarp>,
}

/// Skew or perspective applied to the laid-out text of a box.
#[revisioned(revision = 1)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TextWarp {
    /// Horizontal and vertical shear in degrees.
    Skew { x_degrees: f32, y_degrees: f32 },
    /// Where the frame's corners go in frame-local units, clockwise from the top-left;
    /// the unit square leaves the text unchanged.
    Perspective { corners: [[f32; 2]; 4] },
}

impl TextWarp {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Skew {
                x_degrees,
                y_degrees,
            } => {
                if [x_degrees, y_degrees]
                    .into_iter()
                    .any(|value| !value.is_finite() || !(-60.0..=60.0).contains(value))
                {
                    return Err(Error::invalid(
                        "text skew must be finite and within -60..=60 degrees",
                    ));
                }
            }
            Self::Perspective { corners } => {
                if corners
                    .iter()
                    .flatten()
                    .any(|value| !value.is_finite() || !(-1.0..=2.0).contains(value))
                {
                    return Err(Error::invalid(
                        "text perspective corners must be finite and within -1..=2",
                    ));
                }
                for index in 0..4 {
                    let [a, b, c] =
                        [0, 1, 2].map(|offset| corners[(index + offset) % 4].map(f64::from));
                    let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
                    if cross <= 0.0 {
                        return Err(Error::invalid(
                            "text perspective must be a convex quad with positive area",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

impl Component for TextLayout {
    const KIND: &'static str = "dev.koharu.layer.text";

    fn validate(&self, _context: &ValidationContext<'_>) -> Result<()> {
        if self.angle_degrees.is_some_and(|angle| !angle.is_finite()) {
            return Err(Error::invalid("text layout angle must be finite"));
        }
        if let Some(warp) = &self.warp {
            warp.validate()?;
        }
        self.origin.validate()
    }

    fn origin(&self) -> Option<&Origin> {
        Some(&self.origin)
    }

    fn set_origin(&mut self, origin: Origin) -> bool {
        self.origin = origin;
        true
    }
}

#[revisioned(revision = 1)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
pub enum TextAlignment {
    Start,
    Center,
    End,
    Justify,
}

#[revisioned(revision = 1)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
pub enum WritingMode {
    Horizontal,
    Vertical,
}

#[revisioned(revision = 1)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

#[revisioned(revision = 1)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StrokeMethod {
    Expansion,
    Contour,
}

#[revisioned(revision = 1)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StrokePosition {
    Outside,
    Center,
    Inside,
}

#[revisioned(revision = 1)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StrokeJoin {
    Round,
    Miter,
    Bevel,
}

#[revisioned(revision = 1)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
pub enum GradientKind {
    Linear,
    Radial,
}

#[revisioned(revision = 1)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct GradientStop {
    pub offset: f32,
    pub color: [u8; 4],
}

#[revisioned(revision = 1)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct FillGradient {
    pub kind: GradientKind,
    pub angle_degrees: f32,
    pub stops: Vec<GradientStop>,
}

#[revisioned(revision = 4)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct Typography {
    pub origin: Origin,
    pub preferred_font: Option<String>,
    pub font_weight: Option<u16>,
    pub font_style: Option<FontStyle>,
    pub size: Option<f32>,
    pub auto_fit: bool,
    pub color: Option<[u8; 4]>,
    pub stroke_color: Option<[u8; 4]>,
    pub stroke_width: Option<f32>,
    pub alignment: Option<TextAlignment>,
    pub writing_mode: Option<WritingMode>,
    pub extensions: BTreeMap<String, String>,
    #[revision(start = 2)]
    pub stroke_method: Option<StrokeMethod>,
    #[revision(start = 2)]
    pub stroke_position: Option<StrokePosition>,
    #[revision(start = 2)]
    pub stroke_join: Option<StrokeJoin>,
    /// Color of a second outline outside the first (#698).
    #[revision(start = 3)]
    pub outer_stroke_color: Option<[u8; 4]>,
    /// Width of the second outline; absent or zero means none.
    #[revision(start = 3)]
    pub outer_stroke_width: Option<f32>,
    #[revision(start = 4)]
    pub line_spacing: Option<f32>,
    #[revision(start = 4)]
    pub letter_spacing: Option<f32>,
    #[revision(start = 4)]
    pub fill_gradient: Option<FillGradient>,
}

impl Component for Typography {
    const KIND: &'static str = "dev.koharu.text.typography";

    fn validate(&self, _context: &ValidationContext<'_>) -> Result<()> {
        if self
            .preferred_font
            .as_ref()
            .is_some_and(|font| font.len() > 4096)
            || self
                .font_weight
                .is_some_and(|weight| !(1..=1000).contains(&weight))
            || self
                .size
                .is_some_and(|size| !size.is_finite() || size <= 0.0)
            || (!self.auto_fit && self.size.is_none())
            || self
                .stroke_width
                .is_some_and(|width| !width.is_finite() || width < 0.0)
            || self
                .outer_stroke_width
                .is_some_and(|width| !width.is_finite() || width < 0.0)
            || self
                .line_spacing
                .is_some_and(|value| !value.is_finite() || !(0.5..=3.0).contains(&value))
            || self
                .letter_spacing
                .is_some_and(|value| !value.is_finite() || !(-500.0..=2000.0).contains(&value))
        {
            return Err(Error::invalid("typography intent is invalid"));
        }
        self.origin.validate()?;
        if let Some(gradient) = &self.fill_gradient
            && (!gradient.angle_degrees.is_finite()
                || !(2..=8).contains(&gradient.stops.len())
                || gradient
                    .stops
                    .iter()
                    .any(|stop| !stop.offset.is_finite() || !(0.0..=1.0).contains(&stop.offset))
                || gradient
                    .stops
                    .windows(2)
                    .any(|stops| stops[0].offset > stops[1].offset))
        {
            return Err(Error::invalid("typography gradient is invalid"));
        }
        if self.extensions.len() > 1024
            || self.extensions.iter().any(|(key, value)| {
                validate_namespaced(key, "typography extension").is_err()
                    || value.len() > 64 * 1024
                    || value.contains('\0')
            })
        {
            return Err(Error::invalid("typography extensions are invalid"));
        }
        Ok(())
    }

    fn origin(&self) -> Option<&Origin> {
        Some(&self.origin)
    }

    fn set_origin(&mut self, origin: Origin) -> bool {
        self.origin = origin;
        true
    }
}
