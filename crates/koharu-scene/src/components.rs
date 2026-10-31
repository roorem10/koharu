//! Persisted component values grouped by the responsibility that owns them.
//!
//! This module contains data and local validation only. Cross-component
//! document rules belong to `schema`; resolved views and edit intents belong
//! to `document`.

mod analysis;
mod assets;
mod groups;
mod layers;
mod provenance;
mod rich_text;
mod spatial;
mod structure;
mod text;

pub use analysis::{
    DetectionAnalysis, DetectionLabel, OcrAnalysis, Region, RegionKind, TextDirection,
};
pub(crate) use assets::Assets;
pub use assets::{Asset, AssetInput, AssetMetadata, AssetRole};
pub use groups::{Group, TextGroup};
pub use layers::{
    FillGradient, FontStyle, GradientKind, GradientStop, RasterLayer, RasterLayerKind, StrokeJoin,
    StrokeMethod, StrokePosition, TextAlignment, TextLayout, TextLayoutKind, TextWarp, Typography,
    WritingMode,
};
pub use provenance::{Authored, Generation, Origin};
pub use rich_text::{RunStyle, StyleRun, TextRuns, char_range_to_byte_range};
pub use spatial::{Geometry, Point, Visibility};
pub use structure::{EntityOrigin, Page, PageDraft, Project, Relation, RelationKind};
pub use text::{LanguageTag, SourceText, TextContent, TextRole, Translation};
