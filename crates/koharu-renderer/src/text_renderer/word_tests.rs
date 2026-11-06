use super::*;
use crate::fonts::{FontRequest, FontSystem};
use koharu_rasterizer::{RasterOptions, Rasterizer};
use koharu_scene::{EntityId, GradientStop, RunStyle, StyleRun};

fn descriptor(text: &str) -> TextNodeDescriptor {
    TextNodeDescriptor {
        entity: EntityId::new(),
        text: text.to_owned(),
        language: None,
        width: 260.0,
        height: 120.0,
        warp: None,
        balloon_contour: None,
        preferred_font: None,
        font_families: Vec::new(),
        font_weight: None,
        font_style: None,
        font_size: Some(40.0),
        minimum_font_size: 9.0,
        auto_fit: false,
        normal_hyphenation: false,
        alignment: TextAlign::Left,
        writing_mode: WritingMode::Horizontal,
        foreground_color: [0, 0, 0, 255],
        fill_gradient: None,
        stroke: None,
        line_height: 1.2,
        letter_spacing: 0.0,
        word_spacing: 0.0,
        text_inset: [0.0; 4],
        point_text: true,
        runs: Vec::new(),
    }
}

fn gradient() -> FillGradient {
    FillGradient {
        kind: GradientKind::Linear,
        angle_degrees: 0.0,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: [255, 0, 0, 255],
            },
            GradientStop {
                offset: 1.0,
                color: [0, 0, 255, 255],
            },
        ],
    }
}

fn pixels(scene: PreparedScene, resources: Vec<PreparedResource>) -> image::RgbaImage {
    let frame = super::tests::text_bundle(scene, resources, 300, 180)
        .into_frame_with_raster_sources(&Default::default())
        .unwrap();
    Rasterizer::new()
        .unwrap()
        .rasterize(&frame, RasterOptions::default())
        .unwrap()
        .image
}

#[tokio::test]
async fn unstyled_descriptor_is_byte_identical_to_the_original_painter() {
    let font = FontSystem::new().first_font().unwrap();
    let fonts = Fonts::new();
    fonts
        .prepare(&[FontRequest {
            family: font.family_name().to_owned(),
            weight: None,
            style: None,
        }])
        .await
        .unwrap();
    let mut descriptor = descriptor("Golden");
    descriptor.preferred_font = Some(font.family_name().to_owned());
    let layout = TextLayout::new(&font)
        .with_font_size(40.0)
        .with_alignment(TextAlign::Left)
        .run(&descriptor.text)
        .unwrap();
    let transform = Affine::translate((
        f64::from(layout.placement_offset_x()),
        f64::from(layout.placement_offset_y()),
    ));
    for method in [StrokeMethod::Contour, StrokeMethod::Expansion] {
        for kind in 0..4 {
            descriptor.fill_gradient = (kind == 1).then(gradient);
            descriptor.stroke = (kind >= 2).then_some(StrokeOptions {
                color: [250, 250, 250, 255],
                width_px: 2.0,
                method,
                position: StrokePosition::Outside,
                join: StrokeJoin::Round,
                outer: (kind == 3).then_some(OuterStroke {
                    color: [20, 100, 240, 255],
                    width_px: 2.0,
                }),
            });
            let options = TextRenderOptions {
                color: descriptor
                    .fill_gradient
                    .as_ref()
                    .map_or(descriptor.foreground_color, |gradient| {
                        gradient.stops[0].color
                    }),
                fill_gradient: descriptor.fill_gradient.clone(),
                stroke: descriptor.stroke,
                ..Default::default()
            };
            let mut reference = PreparedScene::default();
            let mut resources = Vec::new();
            TextRenderer::new()
                .render(
                    &mut reference,
                    &mut resources,
                    &layout,
                    WritingMode::Horizontal,
                    &options,
                    transform,
                )
                .unwrap();
            let rendered = TextRenderer::new()
                .render_descriptor(&descriptor, &fonts)
                .unwrap();
            assert_eq!(
                *rendered.scene, reference,
                "portable scene changed for {method:?}, {kind}"
            );
            assert_eq!(
                pixels((*rendered.scene).clone(), rendered.resources.to_vec()),
                pixels(reference, resources)
            );
        }
    }
}

#[test]
fn red_word_and_black_neighbour_remain_above_both_outline_passes() {
    let font = FontSystem::new().first_font().unwrap();
    let layout = TextLayout::new(&font)
        .with_font_size(40.0)
        .run("MM")
        .unwrap();
    let mut descriptor = descriptor("MM");
    descriptor.runs = vec![StyleRun {
        start: 0,
        end: 1,
        style: RunStyle {
            color: Some([255, 0, 0, 255]),
            stroke_width: Some(24.0),
            stroke_color: Some([0, 255, 0, 255]),
            outer_stroke_width: Some(4.0),
            outer_stroke_color: Some([0, 0, 255, 255]),
            ..Default::default()
        },
    }];
    let transform = Affine::translate((35.0, 35.0));
    let mut scene = PreparedScene::default();
    word_paint::render(
        &mut scene,
        &layout,
        &descriptor,
        &TextRenderOptions::default(),
        transform,
        None,
    )
    .unwrap();
    let colors: Vec<_> = scene
        .commands
        .iter()
        .map(|command| match command {
            PreparedSceneCommand::FillPath(path) => path.color,
            _ => panic!("flat run paint is a path"),
        })
        .collect();
    assert_eq!(
        colors,
        vec![
            [0, 0, 255, 255],
            [0, 255, 0, 255],
            [255, 0, 0, 255],
            [0, 0, 0, 255]
        ]
    );
    let actual = pixels(scene, Vec::new());
    let mut plain = PreparedScene::default();
    word_paint::render(
        &mut plain,
        &layout,
        &super::word_tests::descriptor("MM"),
        &TextRenderOptions::default(),
        transform,
        None,
    )
    .unwrap();
    let mask = pixels(plain, Vec::new());
    let boundary = (35.0 + layout.lines[0].glyphs[0].x_advance).floor() as u32;
    let mut black = 0;
    let mut red = 0;
    for (x, y, pixel) in actual.enumerate_pixels() {
        red += usize::from(pixel.0 == [255, 0, 0, 255]);
        if x >= boundary && mask.get_pixel(x, y).0 == [0, 0, 0, 255] {
            assert_eq!(pixel.0, [0, 0, 0, 255]);
            black += 1;
        }
    }
    assert!(red > 20 && black > 20);
}

#[test]
fn gradients_cover_only_their_run_even_across_different_glyph_sizes() {
    let font = FontSystem::new().first_font().unwrap();
    let mut layout = TextLayout::new(&font)
        .with_font_size(40.0)
        .run("MMMM")
        .unwrap();
    layout.lines[0].glyphs[2].font_size = 20.0;
    let mut descriptor = descriptor("MMMM");
    descriptor.runs = vec![StyleRun {
        start: 1,
        end: 3,
        style: RunStyle {
            fill_gradient: Some(gradient()),
            ..Default::default()
        },
    }];
    let mut scene = PreparedScene::default();
    word_paint::render(
        &mut scene,
        &layout,
        &descriptor,
        &Default::default(),
        Affine::IDENTITY,
        None,
    )
    .unwrap();
    let gradients: Vec<_> = scene
        .commands
        .iter()
        .filter_map(|command| match command {
            PreparedSceneCommand::GradientPath { start, end, .. } => Some((*start, *end)),
            _ => None,
        })
        .collect();
    assert_eq!(gradients.len(), 2);
    assert_eq!(gradients[0], gradients[1]);
    assert!(gradients[0].0[0] > 10.0);
    assert!(gradients[0].1[0] - gradients[0].0[0] < f64::from(layout.width) * 0.65);
    let mut glyph_scene = PreparedScene::default();
    let mut resources = Vec::new();
    draw_layout(
        &mut glyph_scene,
        &mut resources,
        &layout,
        WritingMode::Horizontal,
        &Default::default(),
        Affine::IDENTITY,
        GlyphPaint {
            color: [0, 0, 0, 255],
            dilation_px: 0.0,
        },
    );
    let sizes: Vec<_> = glyph_scene
        .commands
        .iter()
        .filter_map(|command| match command {
            PreparedSceneCommand::GlyphRun(run) => Some(run.font_size),
            _ => None,
        })
        .collect();
    assert_eq!(sizes, vec![40.0, 20.0, 40.0]);
}

#[test]
fn cluster_boxes_keep_unicode_ranges_line_ends_rotation_skew_and_perspective() {
    let font = FontSystem::new().first_font().unwrap();
    let descriptor = descriptor("Aé\nCD");
    let layout = TextLayout::new(&font)
        .with_font_size(40.0)
        .run(&descriptor.text)
        .unwrap();
    let original = glyphs::boxes(&descriptor, &layout, &font, Affine::IDENTITY).unwrap();
    assert_eq!(
        original
            .iter()
            .map(|glyph| (glyph.start, glyph.end, glyph.line))
            .collect::<Vec<_>>(),
        vec![
            (0, 1, 0),
            (1, 2, 0),
            (2, 2, 0),
            (3, 4, 1),
            (4, 5, 1),
            (5, 5, 1)
        ]
    );
    assert_eq!(original[2].quad[0], original[2].quad[1]);
    assert!(original[3].quad[0][1] > original[0].quad[0][1]);
    for affine in [
        Affine::rotate(0.4),
        crate::warp::skew(
            Some(&TextWarp::Skew {
                x_degrees: 25.0,
                y_degrees: 0.0,
            }),
            descriptor.width,
            descriptor.height,
        ),
    ] {
        let mapped = glyphs::boxes(&descriptor, &layout, &font, affine).unwrap();
        for (before, after) in original.iter().zip(mapped) {
            for (point, actual) in before.quad.into_iter().zip(after.quad) {
                let expected =
                    affine * vello::kurbo::Point::new(f64::from(point[0]), f64::from(point[1]));
                assert!((expected.x as f32 - actual[0]).abs() < 0.001);
                assert!((expected.y as f32 - actual[1]).abs() < 0.001);
            }
        }
    }
    let corners = [[0.1, 0.0], [0.9, 0.1], [1.0, 1.0], [0.0, 0.9]];
    let warped = TextNodeDescriptor {
        warp: Some(TextWarp::Perspective { corners }),
        ..descriptor.clone()
    };
    let perspective =
        crate::warp::Perspective::new(corners, descriptor.width, descriptor.height).unwrap();
    let mapped = glyphs::boxes(&warped, &layout, &font, Affine::IDENTITY).unwrap();
    for (before, after) in original.iter().zip(mapped) {
        for (point, actual) in before.quad.into_iter().zip(after.quad) {
            let expected = perspective
                .point((f64::from(point[0]), f64::from(point[1])).into())
                .unwrap();
            assert!((expected.x as f32 - actual[0]).abs() < 0.001);
            assert!((expected.y as f32 - actual[1]).abs() < 0.001);
        }
    }
}

#[test]
fn small_word_measurement_uses_the_smallest_drawn_size() {
    let font = FontSystem::new().first_font().unwrap();
    let mut descriptor = descriptor("ABC");
    descriptor.runs = vec![StyleRun {
        start: 1,
        end: 2,
        style: RunStyle {
            size_scale: Some(0.2),
            ..Default::default()
        },
    }];
    let mut layout = TextLayout::new(&font)
        .with_font_size(40.0)
        .run("ABC")
        .unwrap();
    layout.lines[0].glyphs[1].font_size = 8.0;
    assert_eq!(glyphs::smallest_size(&descriptor, &layout), 8.0);
}

#[test]
fn inside_and_centred_word_borders_remain_visible_below_the_fills() {
    let font = FontSystem::new().first_font().unwrap();
    let layout = TextLayout::new(&font)
        .with_font_size(60.0)
        .run("M")
        .unwrap();
    for position in [StrokePosition::Inside, StrokePosition::Center] {
        let mut descriptor = descriptor("M");
        descriptor.runs = vec![StyleRun {
            start: 0,
            end: 1,
            style: RunStyle::default(),
        }];
        let options = TextRenderOptions {
            stroke: Some(StrokeOptions {
                color: [255, 0, 0, 255],
                width_px: 2.0,
                method: StrokeMethod::Contour,
                position,
                join: StrokeJoin::Round,
                outer: None,
            }),
            ..Default::default()
        };
        let mut scene = PreparedScene::default();
        word_paint::render(
            &mut scene,
            &layout,
            &descriptor,
            &options,
            Affine::translate((10.0, 10.0)),
            None,
        )
        .unwrap();
        let image = pixels(scene, Vec::new());
        assert!(
            image
                .pixels()
                .filter(|pixel| pixel.0 == [255, 0, 0, 255])
                .count()
                > 10
        );
        assert!(
            image
                .pixels()
                .filter(|pixel| pixel.0 == [0, 0, 0, 255])
                .count()
                > 10
        );
    }
}

#[test]
fn editable_runs_use_each_glyphs_resolved_face_and_size() {
    let mut system = FontSystem::new();
    let primary = system.first_font().unwrap();
    let alternate = system
        .query_family("DejaVu Serif")
        .unwrap_or_else(|_| primary.clone());
    let mut layout = TextLayout::new(&primary)
        .with_font_size(40.0)
        .run("ABC")
        .unwrap();
    layout.lines[0].glyphs[1].font = &alternate;
    layout.lines[0].glyphs[1].font_size = 20.0;
    let mut descriptor = descriptor("ABC");
    descriptor.runs = vec![StyleRun {
        start: 1,
        end: 2,
        style: RunStyle {
            color: Some([255, 0, 0, 255]),
            size_scale: Some(0.5),
            ..Default::default()
        },
    }];
    let runs = glyphs::style_runs(&descriptor, &layout, &primary);
    assert_eq!(runs.len(), 3);
    assert_eq!((runs[1].start, runs[1].end), (1, 2));
    assert_eq!(runs[1].post_script_font, alternate.post_script_name());
    assert_eq!(runs[1].font_size, 20.0);
    assert_eq!(runs[1].color, [255, 0, 0, 255]);
}

#[tokio::test]
async fn perspective_is_applied_after_word_paint_and_keeps_a_plain_editable_preview() {
    let family = FontSystem::new()
        .first_font()
        .unwrap()
        .family_name()
        .to_owned();
    let fonts = Fonts::new();
    fonts
        .prepare(&[FontRequest {
            family: family.clone(),
            weight: None,
            style: None,
        }])
        .await
        .unwrap();
    let mut descriptor = descriptor("MM");
    descriptor.preferred_font = Some(family);
    descriptor.warp = Some(TextWarp::Perspective {
        corners: [[0.15, 0.0], [0.85, 0.0], [1.0, 1.0], [0.0, 1.0]],
    });
    descriptor.runs = vec![StyleRun {
        start: 0,
        end: 1,
        style: RunStyle {
            color: Some([255, 0, 0, 255]),
            stroke_width: Some(2.0),
            stroke_color: Some([0, 255, 0, 255]),
            ..Default::default()
        },
    }];
    let rendered = TextRenderer::new()
        .render_descriptor(&descriptor, &fonts)
        .unwrap();
    assert!(rendered.unwarped.is_some());
    let editable = rendered.editable.unwrap();
    let plain = pixels((*editable.scene).clone(), editable.resources.to_vec());
    assert!(!plain.pixels().any(|pixel| pixel.0 == [0, 255, 0, 255]));
    let warped = pixels((*rendered.scene).clone(), rendered.resources.to_vec());
    assert!(warped.pixels().any(|pixel| pixel.0 == [255, 0, 0, 255]));
    assert!(warped.pixels().any(|pixel| pixel.0 == [0, 0, 0, 255]));
    assert!(warped.pixels().any(|pixel| pixel.0 == [0, 255, 0, 255]));
}
