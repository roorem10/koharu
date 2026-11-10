//! Fonts, colors, outlines, saved text styles and word styling.

use super::*;

pub(super) fn tools() -> Vec<Tool> {
    vec![
            definition::<SetTypography>(
                "set_typography",
                "Replace an element's typography settings. Font is a family name from available_fonts.",
            ),
            text_style_definition(),
            definition::<crate::commands::emphasis::agent::StyleWords>(
                "style_words",
                "Style words in one text layer, in one undoable edit. Choose text (first whole-word/phrase match; plain match for CJK) or start/end (Unicode character offsets, end exclusive). Choose style: emphasis for its category preset, explicit bold/italic/size_percent/color/outline_color/outline_width, or clear: true. Colors are RGBA byte arrays. Word styles remain editable. Example: {\"layer\":\"<ID>\",\"text\":\"NO\",\"size_percent\":130}.",
            ),
    ]
}

pub(super) async fn invoke(
    host: &KoharuHost,
    call: &ToolCall,
    _control: &Control,
) -> Result<Option<Invocation>> {
    let result: Result<Invocation> = match call.name.as_str() {
        "set_typography" => {
            let arguments: SetTypography = arguments(call)?;
            let element = entity(&arguments.element)?;
            let typography = Typography {
                preferred_font: arguments.preferred_font,
                font_weight: arguments.font_weight,
                font_style: arguments.font_style.map(Into::into),
                size: arguments.size,
                auto_fit: arguments.auto_fit,
                color: arguments.color,
                stroke_color: arguments.stroke_color,
                stroke_width: arguments.stroke_width,
                stroke_method: arguments.stroke_method.map(|value| match value {
                    AgentStrokeMethod::Expansion => koharu_scene::StrokeMethod::Expansion,
                    AgentStrokeMethod::Contour => koharu_scene::StrokeMethod::Contour,
                }),
                stroke_position: arguments.stroke_position.map(|value| match value {
                    AgentStrokePosition::Outside => koharu_scene::StrokePosition::Outside,
                    AgentStrokePosition::Center => koharu_scene::StrokePosition::Center,
                    AgentStrokePosition::Inside => koharu_scene::StrokePosition::Inside,
                }),
                stroke_join: arguments.stroke_join.map(|value| match value {
                    AgentStrokeJoin::Round => koharu_scene::StrokeJoin::Round,
                    AgentStrokeJoin::Miter => koharu_scene::StrokeJoin::Miter,
                    AgentStrokeJoin::Bevel => koharu_scene::StrokeJoin::Bevel,
                }),
                outer_stroke_color: arguments.outer_stroke_color,
                outer_stroke_width: arguments.outer_stroke_width,
                line_spacing: arguments.line_spacing,
                letter_spacing: arguments.letter_spacing,
                fill_gradient: arguments.fill_gradient,
                alignment: arguments.alignment.map(Into::into),
                writing_mode: arguments.writing_mode.map(Into::into),
            };
            host.mutate(|project| {
                Box::pin(async move {
                    Ok((
                        project
                            .set_typography(vec![TypographyUpdate {
                                layer: element,
                                typography,
                            }])
                            .await?,
                        json!({ "element": element }),
                    ))
                })
            })
            .await
        }
        "apply_text_style" => {
            let arguments: ApplyTextStyle = arguments(call)?;
            let styles = crate::commands::styles::load_text_styles()?.read()?.clone();
            let detect_colors = koharu_pipeline::PipelineConfig::load()?
                .read()?
                .detect_text_color;
            host.mutate_optional(|project| {
                Box::pin(async move {
                    apply_agent_text_style(project, arguments, &styles, detect_colors).await
                })
            })
            .await
        }
        "style_words" => {
            let arguments: crate::commands::emphasis::agent::StyleWords = arguments(call)?;
            host.mutate(|project| {
                Box::pin(async move {
                    let commit = project.style_words(arguments).await?;
                    Ok((commit, json!({ "styled": true })))
                })
            })
            .await
        }
        _ => return Ok(None),
    };
    result.map(Some)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ApplyTextStyle {
    elements: Vec<String>,
    /// One of rectangle, round, spiky, none. Applies the default and the shape's style.
    shape: String,
}

fn text_style_definition() -> Tool {
    let mut tool = definition::<ApplyTextStyle>(
        "apply_text_style",
        "Restyle text elements as if their bubble had another shape. Use after view_page shows that a bubble was sorted wrongly, for example a shout in a bubble detected as round.",
    );
    tool.parameters["properties"]["shape"] =
        json!({"type": "string", "enum": ["rectangle", "round", "spiky", "none"]});
    tool
}

async fn apply_agent_text_style(
    project: &mut Project,
    arguments: ApplyTextStyle,
    styles: &koharu_renderer::TextStyles,
    detect_colors: bool,
) -> Result<(Option<Commit>, crate::commands::styles::StyleApplyResult)> {
    use crate::commands::styles::StyleScope;
    let scope = StyleScope::Layers(entities(&arguments.elements)?);
    let shape_style = match arguments.shape.as_str() {
        "rectangle" => &styles.shape_styles.rectangle,
        "round" => &styles.shape_styles.round,
        "spiky" => &styles.shape_styles.spiky,
        "none" => &styles.shape_styles.free,
        shape => bail!("unknown shape {shape}; valid shapes: rectangle, round, spiky, none"),
    };
    styles.validate()?;
    project
        .apply_style(scope, true, true, |_, _, typography| {
            styles
                .default_style
                .apply_to_detected(typography, detect_colors);
            shape_style.apply_to_detected(typography, detect_colors);
            Ok(())
        })
        .await
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentTextAlignment {
    Start,
    Center,
    End,
    Justify,
}

impl From<AgentTextAlignment> for koharu_scene::TextAlignment {
    fn from(value: AgentTextAlignment) -> Self {
        match value {
            AgentTextAlignment::Start => Self::Start,
            AgentTextAlignment::Center => Self::Center,
            AgentTextAlignment::End => Self::End,
            AgentTextAlignment::Justify => Self::Justify,
        }
    }
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentWritingMode {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentFontStyle {
    Normal,
    Italic,
    Oblique,
}

impl From<AgentFontStyle> for koharu_scene::FontStyle {
    fn from(value: AgentFontStyle) -> Self {
        match value {
            AgentFontStyle::Normal => Self::Normal,
            AgentFontStyle::Italic => Self::Italic,
            AgentFontStyle::Oblique => Self::Oblique,
        }
    }
}

impl From<AgentWritingMode> for koharu_scene::WritingMode {
    fn from(value: AgentWritingMode) -> Self {
        match value {
            AgentWritingMode::Horizontal => Self::Horizontal,
            AgentWritingMode::Vertical => Self::Vertical,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentStrokeMethod {
    Expansion,
    Contour,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentStrokePosition {
    Outside,
    Center,
    Inside,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentStrokeJoin {
    Round,
    Miter,
    Bevel,
}

#[derive(Deserialize, JsonSchema)]
struct SetTypography {
    element: String,
    preferred_font: Option<String>,
    font_weight: Option<u16>,
    font_style: Option<AgentFontStyle>,
    size: Option<f32>,
    auto_fit: bool,
    color: Option<[u8; 4]>,
    stroke_color: Option<[u8; 4]>,
    stroke_width: Option<f32>,
    stroke_method: Option<AgentStrokeMethod>,
    stroke_position: Option<AgentStrokePosition>,
    stroke_join: Option<AgentStrokeJoin>,
    /// Color of a second outline outside the first.
    outer_stroke_color: Option<[u8; 4]>,
    /// Width of the second outline in pixels; omit or 0 for none.
    outer_stroke_width: Option<f32>,
    line_spacing: Option<f32>,
    letter_spacing: Option<f32>,
    #[schemars(with = "Option<AgentFillGradient>")]
    fill_gradient: Option<koharu_scene::FillGradient>,
    alignment: Option<AgentTextAlignment>,
    writing_mode: Option<AgentWritingMode>,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct AgentFillGradient {
    kind: AgentGradientKind,
    angle_degrees: f32,
    stops: Vec<AgentGradientStop>,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
enum AgentGradientKind {
    Linear,
    Radial,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct AgentGradientStop {
    offset: f32,
    color: [u8; 4],
}

#[cfg(test)]
mod text_style_tests {
    use super::*;
    use koharu_renderer::{FontPart, ShapeStyles, TextStyle, TextStyles};
    use koharu_scene::{
        At, BubbleRegion, FlowsIn, Generation, Geometry, Origin, PageDraft, Point, ProducerId,
        TextLayout, TextLayoutKind,
    };

    #[tokio::test]
    async fn context_and_forced_shape_style_share_the_real_batch_and_undo() {
        let directory = tempfile::tempdir().unwrap();
        let mut project =
            Project::create("agent styles".into(), directory.path().join("project.khr"))
                .await
                .unwrap();
        let generation = Generation::new(ProducerId::new("dev.koharu.pipeline.detection").unwrap());
        let mut edit = project.snapshot().edit_as(generation.clone());
        let page = edit
            .add_page(PageDraft::new("Page 3", 300.0, 300.0), At::End)
            .unwrap();
        let points = (0..24)
            .map(|index| {
                let radius = if index % 2 == 0 { 100.0 } else { 60.0 };
                let angle = index as f64 * std::f64::consts::TAU / 24.0;
                Point {
                    x: 150.0 + radius * angle.cos(),
                    y: 150.0 + radius * angle.sin(),
                }
            })
            .collect();
        let bubble = edit
            .add_analysis_region::<BubbleRegion>(
                page,
                At::End,
                &Geometry {
                    origin: Origin::Generated(generation.clone()),
                    points,
                },
                None,
            )
            .unwrap();
        let mut layers = Vec::new();
        for _ in 0..2 {
            let content = edit.add_text_content(page, At::End).unwrap();
            let layer = edit
                .add_text_layer(
                    page,
                    At::End,
                    content,
                    &TextLayout {
                        origin: Origin::Generated(generation.clone()),
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: None,
                        warp: None,
                    },
                )
                .unwrap();
            edit.set(
                layer,
                &koharu_scene::Typography {
                    origin: Origin::Generated(generation.clone()),
                    preferred_font: Some("Original".into()),
                    font_weight: None,
                    font_style: None,
                    size: None,
                    auto_fit: true,
                    color: Some([1, 2, 3, 255]),
                    stroke_color: None,
                    stroke_width: None,
                    stroke_method: None,
                    stroke_position: None,
                    stroke_join: None,
                    outer_stroke_color: None,
                    outer_stroke_width: None,
                    line_spacing: None,
                    letter_spacing: None,
                    fill_gradient: None,
                    alignment: None,
                    writing_mode: Some(koharu_scene::WritingMode::Vertical),
                    extensions: Default::default(),
                },
            )
            .unwrap();
            edit.relate::<FlowsIn>(layer, bubble).unwrap();
            layers.push(layer);
        }
        project
            .commit_rebased(edit.finish().unwrap())
            .await
            .unwrap()
            .unwrap();
        let original = project
            .snapshot()
            .component::<koharu_scene::Typography>(layers[0])
            .unwrap()
            .unwrap();
        let first_text = |pages: &[Value]| {
            pages[0]["layers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|layer| layer["type"] == "text")
                .unwrap()
                .clone()
        };
        let pages = agent_pages(&project.snapshot()).unwrap();
        assert_eq!(first_text(&pages)["bubble_shape"], "spiky");
        assert_eq!(first_text(&pages)["style_origin"], "generated");
        assert!(
            first_text(&pages)["shape_measures"]["spikes"]
                .as_u64()
                .unwrap()
                >= 5
        );
        let styles = TextStyles {
            shape_styles: ShapeStyles {
                spiky: TextStyle {
                    font: Some(FontPart {
                        preferred_font: Some("Spiky Font".into()),
                        font_weight: None,
                        font_style: None,
                    }),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let configuration = text_styles_context(&styles);
        assert_eq!(
            configuration["shape_styles"]["spiky"]["font"]["preferred_font"],
            "Spiky Font"
        );
        assert!(configuration.get("presets").is_none());
        let args = || ApplyTextStyle {
            elements: layers.iter().map(ToString::to_string).collect(),
            shape: "spiky".into(),
        };
        let (commit, result) = apply_agent_text_style(&mut project, args(), &styles, true)
            .await
            .unwrap();
        assert_eq!((result.changed, result.skipped), (2, 0));
        project.record_commit(&commit.unwrap());
        assert_eq!(project.undo.len(), 1);
        for layer in &layers {
            let typography = project
                .snapshot()
                .component::<koharu_scene::Typography>(*layer)
                .unwrap()
                .unwrap();
            assert_eq!(typography.preferred_font.as_deref(), Some("Spiky Font"));
            assert_eq!(typography.origin, Origin::User);
            assert_eq!(typography.color, original.color);
            assert_eq!(typography.writing_mode, original.writing_mode);
        }
        assert_eq!(
            first_text(&agent_pages(&project.snapshot()).unwrap())["style_origin"],
            "user"
        );
        let (commit, result) = apply_agent_text_style(&mut project, args(), &styles, true)
            .await
            .unwrap();
        assert!(commit.is_none());
        assert_eq!((result.changed, result.skipped), (0, 0));
        assert!(!Invocation::read(result).unwrap().changed);
        project.undo().await.unwrap();
        for layer in &layers {
            assert_eq!(
                project
                    .snapshot()
                    .component::<koharu_scene::Typography>(*layer)
                    .unwrap()
                    .unwrap(),
                original
            );
        }
        let mut invalid = args();
        invalid.shape = "triangle".into();
        let error = apply_agent_text_style(&mut project, invalid, &styles, true)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("rectangle, round, spiky, none"));
    }
}
