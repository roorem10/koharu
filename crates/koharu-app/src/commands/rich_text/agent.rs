use anyhow::{Context as _, Result, bail};
use koharu_agent::{Invocation, Tool, ToolCall};
use koharu_scene::EntityId;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Manager as _};
use tauri_runtime_cef::CefRuntime;

use super::RunStylePatch;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetWordStyles {
    #[schemars(with = "String")]
    layer: EntityId,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetWordStyle {
    #[schemars(with = "String")]
    layer: EntityId,
    start: u32,
    end: u32,
    #[schemars(with = "PatchSchema")]
    patch: RunStylePatch,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ClearWordStyles {
    #[schemars(with = "Vec<String>")]
    layers: Vec<EntityId>,
    range: Option<[u32; 2]>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReplaceTextRange {
    #[schemars(with = "String")]
    layer: EntityId,
    start: u32,
    end: u32,
    insert: String,
}

// Scene types own deserialization and validation; these mirror their wire
// shape only because scene components do not implement JsonSchema.
#[derive(JsonSchema)]
#[allow(dead_code)]
struct PatchSchema {
    set: RunStyleSchema,
    clear: Vec<RunStyleFieldSchema>,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct RunStyleSchema {
    font: Option<String>,
    font_weight: Option<u16>,
    font_style: Option<FontStyleSchema>,
    size_scale: Option<f32>,
    color: Option<[u8; 4]>,
    fill_gradient: Option<FillGradientSchema>,
    stroke_color: Option<[u8; 4]>,
    stroke_width: Option<f32>,
    outer_stroke_color: Option<[u8; 4]>,
    outer_stroke_width: Option<f32>,
}

#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
enum FontStyleSchema {
    Normal,
    Italic,
    Oblique,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct FillGradientSchema {
    kind: GradientKindSchema,
    angle_degrees: f32,
    stops: Vec<GradientStopSchema>,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
enum GradientKindSchema {
    Linear,
    Radial,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct GradientStopSchema {
    offset: f32,
    color: [u8; 4],
}

#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
enum RunStyleFieldSchema {
    Font,
    FontWeight,
    FontStyle,
    SizeScale,
    Color,
    FillGradient,
    StrokeColor,
    StrokeWidth,
    OuterStrokeColor,
    OuterStrokeWidth,
}

pub(crate) fn get_word_styles_definition() -> Tool {
    Tool::new(
        "get_word_styles",
        "Read a text layer's word-style runs; offsets count Unicode characters, end exclusive.",
        json!(schema_for!(GetWordStyles)),
    )
}

pub(crate) fn set_word_style_definition() -> Tool {
    Tool::new(
        "set_word_style",
        "Patch any word-style fields; clear restores box inheritance. Unicode offsets, end exclusive; colors are RGBA bytes; size_scale is a multiplier.",
        json!(schema_for!(SetWordStyle)),
    )
}

pub(crate) fn clear_word_styles_definition() -> Tool {
    Tool::new(
        "clear_word_styles",
        "Clear word styles on text layers, optionally within [start,end) Unicode character offsets, in one undo step.",
        json!(schema_for!(ClearWordStyles)),
    )
}

pub(crate) fn replace_text_range_definition() -> Tool {
    Tool::new(
        "replace_text_range",
        "Replace shown text at [start,end) Unicode character offsets, preserving surrounding word styles. Equal offsets insert text.",
        json!(schema_for!(ReplaceTextRange)),
    )
}

pub(crate) async fn dispatch(
    handle: &AppHandle<CefRuntime>,
    call: &ToolCall,
) -> Result<Invocation> {
    match call.name.as_str() {
        "get_word_styles" => {
            let args: GetWordStyles = serde_json::from_str(&call.arguments)
                .context("invalid get_word_styles arguments")?;
            let runs = super::get_word_styles(args.layer, handle.state())
                .await
                .map_err(|error| error.0)?;
            Invocation::read(runs)
        }
        "set_word_style" => {
            let args: SetWordStyle = serde_json::from_str(&call.arguments)
                .context("invalid set_word_style arguments")?;
            super::set_word_style(
                args.layer,
                args.start,
                args.end,
                args.patch,
                handle.state(),
                handle.state(),
                handle.state(),
            )
            .await
            .map_err(|error| error.0)?;
            Invocation::changed(json!({ "layer": args.layer }))
        }
        "clear_word_styles" => {
            let args: ClearWordStyles = serde_json::from_str(&call.arguments)
                .context("invalid clear_word_styles arguments")?;
            super::clear_word_styles(
                args.layers,
                args.range,
                handle.state(),
                handle.state(),
                handle.state(),
            )
            .await
            .map_err(|error| error.0)?;
            Invocation::changed(json!({ "cleared": true }))
        }
        "replace_text_range" => {
            let args: ReplaceTextRange = serde_json::from_str(&call.arguments)
                .context("invalid replace_text_range arguments")?;
            super::replace_text_range(
                args.layer,
                args.start,
                args.end,
                args.insert,
                None,
                handle.state(),
                handle.state(),
                handle.state(),
            )
            .await
            .map_err(|error| error.0)?;
            Invocation::changed(json!({ "layer": args.layer }))
        }
        _ => bail!("unknown rich-text tool {}", call.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_scene::{FontStyle, GradientKind, char_range_to_byte_range, rich_text};

    #[test]
    fn full_style_arguments_preserve_native_fields_and_character_offsets() {
        let layer = EntityId::new();
        let style = json!({
            "font": "Lettering",
            "font_weight": 650,
            "font_style": "oblique",
            "size_scale": 1.25,
            "color": [12, 34, 56, 255],
            "fill_gradient": {
                "kind": "Radial",
                "angle_degrees": 45.0,
                "stops": [
                    { "offset": 0.0, "color": [0, 0, 0, 255] },
                    { "offset": 1.0, "color": [255, 255, 255, 255] }
                ]
            },
            "stroke_color": [100, 110, 120, 255],
            "stroke_width": 2.0,
            "outer_stroke_color": [200, 210, 220, 255],
            "outer_stroke_width": 3.0
        });
        let args: SetWordStyle = serde_json::from_value(json!({
            "layer": layer,
            "start": 1,
            "end": 3,
            "patch": {
                "set": style,
                "clear": ["font_weight", "outer_stroke_color"]
            }
        }))
        .unwrap();
        assert_eq!(args.layer, layer);
        args.patch.set.validate().unwrap();
        assert_eq!(serde_json::to_value(&args.patch.set).unwrap(), style);
        assert_eq!(
            args.patch.clear,
            vec![
                rich_text::RunStyleField::FontWeight,
                rich_text::RunStyleField::OuterStrokeColor
            ]
        );

        let text = "a\u{1f47b}\u{738b}b";
        assert_eq!(
            &text[char_range_to_byte_range(text, args.start, args.end)],
            "\u{1f47b}\u{738b}"
        );
        let styled = rich_text::apply_patch(
            &[],
            args.start,
            args.end,
            &args.patch.set,
            &args.patch.clear,
        );
        assert_eq!(styled.len(), 1);
        assert_eq!((styled[0].start, styled[0].end), (1, 3));
        assert_eq!(styled[0].style.font_weight, None);
        assert_eq!(styled[0].style.outer_stroke_color, None);
        let mut expected = args.patch.set;
        expected.font_weight = None;
        expected.outer_stroke_color = None;
        assert_eq!(styled[0].style, expected);
    }

    #[test]
    fn schema_enum_spellings_match_native_wire_types() {
        assert_eq!(
            json!(schema_for!(GradientKindSchema))["enum"],
            json!([GradientKind::Linear, GradientKind::Radial])
        );
        assert_eq!(
            json!(schema_for!(FontStyleSchema))["enum"],
            json!([FontStyle::Normal, FontStyle::Italic, FontStyle::Oblique])
        );
        use rich_text::RunStyleField;
        assert_eq!(
            json!(schema_for!(RunStyleFieldSchema))["enum"],
            json!([
                RunStyleField::Font,
                RunStyleField::FontWeight,
                RunStyleField::FontStyle,
                RunStyleField::SizeScale,
                RunStyleField::Color,
                RunStyleField::FillGradient,
                RunStyleField::StrokeColor,
                RunStyleField::StrokeWidth,
                RunStyleField::OuterStrokeColor,
                RunStyleField::OuterStrokeWidth
            ])
        );
    }
}
