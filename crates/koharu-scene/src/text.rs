//! Text: styled spans in a frame. A text layer holds one, and so do a block's
//! source and translation.

use std::ops::Range;

use icu_locale_core::LanguageIdentifier;
use kurbo::{Affine, BezPath, Shape as _};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use specta::Type;

use crate::{Align, Direction, Frame, Rgba, Transform};

/// Character values; each is also a mark key on a span, and a mark wins for
/// its span.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharStyle {
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub weight: Option<u16>,
    #[serde(default)]
    pub italic: Option<bool>,
    #[serde(default)]
    pub size: Option<f64>,
    #[serde(default)]
    pub fill: Option<Rgba>,
    #[serde(default)]
    pub stroke: Option<Rgba>,
    #[serde(default)]
    pub stroke_width: Option<f64>,
    /// Booharu: a second border drawn around the first, outside its outer
    /// edge; it shows only with a first border. Left out when unset, so a
    /// text without one stays readable by Koharu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outer_stroke: Option<Rgba>,
    /// Booharu: the second border's width, outward from the first border.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outer_stroke_width: Option<f64>,
    /// Booharu: a gradient fill, drawn in place of `fill`. The two are one
    /// paint: a gradient wins over a `fill` beside it, and a `fill` with no
    /// gradient beside it wins over a gradient it would inherit, so a word
    /// given a colour shows it in a gradient box, and a patch giving a text
    /// with a gradient a flat colour sets this to null beside the `fill`.
    /// Left out when unset, so a text without one stays readable by Koharu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_gradient: Option<Gradient>,
    /// Booharu: letter spacing in thousandths of an em, as Photoshop's
    /// tracking, from -500 to 2000: the space each character is followed by
    /// grows by that much, or shrinks when it's negative. Left out when
    /// unset, so a text without it stays readable by Koharu.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "tracking"
    )]
    #[specta(type = Option<f64>)]
    #[schemars(range(min = -500.0, max = 2000.0))]
    pub tracking: Option<f64>,
}

impl CharStyle {
    /// These values, with `base`'s where these have none.
    #[must_use]
    pub fn or(&self, base: &Self) -> Self {
        Self {
            family: self.family.clone().or_else(|| base.family.clone()),
            weight: self.weight.or(base.weight),
            italic: self.italic.or(base.italic),
            size: self.size.or(base.size),
            fill: self.fill.or(base.fill),
            stroke: self.stroke.or(base.stroke),
            stroke_width: self.stroke_width.or(base.stroke_width),
            // Booharu: the second border.
            outer_stroke: self.outer_stroke.or(base.outer_stroke),
            outer_stroke_width: self.outer_stroke_width.or(base.outer_stroke_width),
            // Booharu: a fill and a gradient are one paint, so a fill set
            // here with no gradient beside it hides the gradient `base` has.
            fill_gradient: if self.fill.is_some() && self.fill_gradient.is_none() {
                None
            } else {
                self.fill_gradient
                    .clone()
                    .or_else(|| base.fill_gradient.clone())
            },
            // Booharu: letter spacing.
            tracking: self.tracking.or(base.tracking),
        }
    }
}

/// Booharu: a letter spacing, which as a tracking of Photoshop's runs from
/// -500 to 2000 thousandths of an em.
fn tracking<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
    let tracking = Option::<f64>::deserialize(deserializer)?;
    match tracking {
        Some(value) if !(-500.0..=2000.0).contains(&value) => Err(serde::de::Error::custom(
            format!("a letter spacing of {value} isn't from -500 to 2000"),
        )),
        _ => Ok(tracking),
    }
}

/// A text's character and paragraph values.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextStyle {
    #[serde(default)]
    pub chars: CharStyle,
    /// A paragraph key: style only, never a mark.
    #[serde(default)]
    pub align: Option<Align>,
    /// A paragraph key: a multiple of each line's largest size.
    #[serde(default)]
    pub line_height: Option<f64>,
    /// A paragraph key, Booharu's: how the borders are drawn; `None` is
    /// Expansion, Koharu's border. Left out when unset, so a text without one
    /// stays readable by Koharu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_method: Option<StrokeMethod>,
}

/// Booharu: how a text's borders are drawn, both outside the letters with
/// round corners. Koharu's own border is the default, so a text with no
/// method, Koharu's or Booharu's, draws as Koharu draws it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StrokeMethod {
    /// Each letter grown on its own, as Koharu draws borders: the default,
    /// and what a text with no method draws.
    #[default]
    Expansion,
    /// Grown from the letters merged into one shape, every run with that
    /// border together, so a translucent border shows no darker overlap
    /// where letters touch, and no border shows through a see-through letter.
    Contour,
}

/// Booharu: a gradient fill: its stops spread along a line at `angle`, or
/// out from the centre, over the ink of the text it fills, or of the run of
/// characters whose mark it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Gradient {
    pub kind: GradientKind,
    /// The direction in degrees, as Photoshop's angle: 0 runs left to
    /// right and 90 bottom to top. A radial gradient ignores it.
    #[serde(default, deserialize_with = "angle")]
    #[specta(type = f64)]
    pub angle: f64,
    /// 2 to 8 stops in order: an offset is never below the one before it,
    /// and two can share one, for a hard edge.
    #[serde(deserialize_with = "stops")]
    #[specta(type = Vec<GradientStop>)]
    #[schemars(length(min = 2, max = 8))]
    pub stops: Vec<GradientStop>,
}

/// Booharu: how a gradient's stops spread.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum GradientKind {
    /// Along a line through the centre, at the gradient's angle.
    Linear,
    /// Out from the centre to a circle through the corners.
    Radial,
}

/// Booharu: one colour of a gradient and where it falls along it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GradientStop {
    /// From 0 at the gradient's start to 1 at its end.
    #[serde(deserialize_with = "offset")]
    #[specta(type = f64)]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub offset: f64,
    pub color: Rgba,
}

/// Booharu: a gradient's angle, any finite number of degrees.
fn angle<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let angle = f64::deserialize(deserializer)?;
    if angle.is_finite() {
        Ok(angle)
    } else {
        Err(serde::de::Error::custom(format!(
            "a gradient's angle of {angle} isn't finite"
        )))
    }
}

/// Booharu: a gradient's stops, 2 to 8 of them, none at an offset below the
/// one before it.
fn stops<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<GradientStop>, D::Error> {
    let stops = Vec::<GradientStop>::deserialize(deserializer)?;
    if !(2..=8).contains(&stops.len()) {
        return Err(serde::de::Error::custom(format!(
            "a gradient has {} stops, not 2 to 8",
            stops.len()
        )));
    }
    match stops
        .windows(2)
        .find(|pair| pair[1].offset < pair[0].offset)
    {
        Some(pair) => Err(serde::de::Error::custom(format!(
            "a gradient's stop at {} follows one at {}",
            pair[1].offset, pair[0].offset
        ))),
        None => Ok(stops),
    }
}

/// Booharu: a gradient stop's offset, from 0 to 1.
fn offset<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let offset = f64::deserialize(deserializer)?;
    if (0.0..=1.0).contains(&offset) {
        Ok(offset)
    } else {
        Err(serde::de::Error::custom(format!(
            "a gradient stop's offset of {offset} isn't from 0 to 1"
        )))
    }
}

/// Text as spans in order, each with its marks, so no mark can fall outside the
/// text. Deserializes from a plain string too.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(from = "RichTextInput")]
#[specta(type = Vec<Span>)]
pub struct RichText(pub Vec<Span>);

/// A string with its character marks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Span {
    pub insert: String,
    #[serde(default, skip_serializing_if = "unmarked")]
    pub attributes: CharStyle,
}

fn unmarked(style: &CharStyle) -> bool {
    *style == CharStyle::default()
}

#[derive(Deserialize, Type, JsonSchema)]
#[serde(untagged)]
enum RichTextInput {
    Plain(String),
    Spans(Vec<Span>),
}

impl From<RichTextInput> for RichText {
    fn from(input: RichTextInput) -> Self {
        match input {
            RichTextInput::Plain(text) => Self::from(text.as_str()),
            RichTextInput::Spans(spans) => Self(spans),
        }
    }
}

impl RichText {
    /// The plain text.
    #[must_use]
    pub fn text(&self) -> String {
        self.0.iter().map(|span| span.insert.as_str()).collect()
    }

    /// `new` carrying these marks on the characters that stayed. A new
    /// character takes the marks of the one before it, kept or replaced, or of
    /// the first character at the start, as typing does in a text editor.
    #[must_use]
    pub fn replaced(&self, new: &str) -> Self {
        let plain = self.text();
        let marks: Vec<&CharStyle> = self
            .0
            .iter()
            .flat_map(|span| span.insert.chars().map(|_| &span.attributes))
            .collect();
        let unmarked = CharStyle::default();
        let mut last = marks.first().copied().unwrap_or(&unmarked);
        let mut consumed = 0;
        let mut spans = Vec::new();
        for change in TextDiff::from_chars(plain.as_str(), new).iter_all_changes() {
            if change.tag() != ChangeTag::Insert {
                last = marks[consumed];
                consumed += 1;
            }
            if change.tag() != ChangeTag::Delete {
                spans.push(Span {
                    insert: change.value().to_owned(),
                    attributes: last.clone(),
                });
            }
        }
        joined(spans)
    }

    /// `range`, UTF-8 offsets on character boundaries, replaced by `insert`
    /// carrying `marks`, as typing over a selection does; the other
    /// characters keep theirs.
    #[must_use]
    pub fn spliced(&self, range: Range<usize>, insert: &str, marks: &CharStyle) -> Self {
        let inserted = Span {
            insert: insert.to_owned(),
            attributes: marks.clone(),
        };
        joined(
            self.slice(0..range.start)
                .chain(std::iter::once(inserted))
                .chain(self.slice(range.end..usize::MAX)),
        )
    }

    /// `range`'s characters with `patch`'s values over their marks.
    #[must_use]
    pub fn marked(&self, range: Range<usize>, patch: &CharStyle) -> Self {
        let restyled = self.slice(range.clone()).map(|span| Span {
            attributes: patch.or(&span.attributes),
            ..span
        });
        joined(
            self.slice(0..range.start)
                .chain(restyled)
                .chain(self.slice(range.end..usize::MAX)),
        )
    }

    /// The marks a character typed at `offset` takes: the one's before it,
    /// or the first character's at the start, as `replaced` gives them.
    #[must_use]
    pub fn marks_at(&self, offset: usize) -> CharStyle {
        let mut start = 0;
        for span in &self.0 {
            let end = start + span.insert.len();
            if offset <= end {
                return span.attributes.clone();
            }
            start = end;
        }
        self.0
            .last()
            .map(|span| span.attributes.clone())
            .unwrap_or_default()
    }

    /// The spans' parts within `range`, UTF-8 offsets on character
    /// boundaries.
    fn slice(&self, range: Range<usize>) -> impl Iterator<Item = Span> + '_ {
        let mut start = 0;
        self.0.iter().filter_map(move |span| {
            let (from, to) = (start, start + span.insert.len());
            start = to;
            let (cut_from, cut_to) = (range.start.max(from), range.end.min(to));
            (cut_from < cut_to).then(|| Span {
                insert: span.insert[cut_from - from..cut_to - from].to_owned(),
                attributes: span.attributes.clone(),
            })
        })
    }
}

impl From<&str> for RichText {
    fn from(text: &str) -> Self {
        Self(match text.is_empty() {
            true => Vec::new(),
            false => vec![Span {
                insert: text.to_owned(),
                attributes: CharStyle::default(),
            }],
        })
    }
}

pub(crate) fn und() -> LanguageIdentifier {
    LanguageIdentifier::UNKNOWN
}

/// Styled text in a frame. `content` is set when its layer is added; after
/// that only the text ops change it, except a block's source, whose wording
/// a patch writes, as OCR does.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Text {
    #[serde(default)]
    pub content: RichText,
    #[serde(default)]
    pub style: TextStyle,
    #[serde(default)]
    pub frame: Frame,
    #[serde(default)]
    pub direction: Direction,
    /// A BCP 47 tag; `und` when undetermined.
    #[serde(default = "und")]
    #[specta(type = String)]
    #[schemars(with = "String")]
    pub language: LanguageIdentifier,
    #[serde(default)]
    pub transform: Transform,
}

impl Text {
    /// Where its frame lies on the page, its transform applied; `None` for
    /// point text.
    #[must_use]
    pub fn bounds(&self) -> Option<kurbo::Rect> {
        let rect = match &self.frame {
            Frame::Point => return None,
            Frame::Box { bounds, .. } => kurbo::Rect::from(*bounds),
            Frame::Path { path, .. } => BezPath::from(path).bounding_box(),
        };
        Some(Affine::from(self.transform).transform_rect_bbox(rect))
    }

    /// Its place and style without its words or their language: what a block's
    /// translation starts as, from its source, whichever writer adds it, so
    /// new lettering lies where the original did.
    #[must_use]
    pub fn blank(&self) -> Self {
        Self {
            content: RichText::default(),
            language: und(),
            ..self.clone()
        }
    }
}

/// Empty point text in an undetermined language.
impl Default for Text {
    fn default() -> Self {
        Self {
            content: RichText::default(),
            style: TextStyle::default(),
            frame: Frame::default(),
            direction: Direction::default(),
            language: und(),
            transform: Transform::default(),
        }
    }
}

/// `spans` with empty ones dropped and equal neighbours joined.
pub(crate) fn joined(spans: impl IntoIterator<Item = Span>) -> RichText {
    let mut out: Vec<Span> = Vec::new();
    for span in spans {
        match out.last_mut() {
            _ if span.insert.is_empty() => {}
            Some(last) if last.attributes == span.attributes => last.insert.push_str(&span.insert),
            _ => out.push(span),
        }
    }
    RichText(out)
}
