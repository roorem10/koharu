use super::*;
use koharu_renderer::{OuterPart, OutlinePart};
use koharu_scene::{
    At, FillGradient, FontStyle, GradientKind, GradientStop, PageDraft, RunStyle, Session,
    StrokeJoin, StrokeMethod, StrokePosition, StyleRun, TextAlignment, TextLayoutKind, TextWarp,
    WritingMode,
};

fn typography() -> Typography {
    Typography {
        origin: Origin::User,
        preferred_font: Some("Dialogue".into()),
        font_weight: Some(500),
        font_style: Some(FontStyle::Italic),
        size: Some(24.0),
        auto_fit: false,
        color: Some([10, 20, 30, 255]),
        stroke_color: None,
        stroke_width: None,
        stroke_method: Some(StrokeMethod::Contour),
        stroke_position: None,
        stroke_join: None,
        outer_stroke_color: None,
        outer_stroke_width: None,
        line_spacing: Some(1.4),
        letter_spacing: Some(-20.0),
        fill_gradient: None,
        alignment: Some(TextAlignment::End),
        writing_mode: Some(WritingMode::Vertical),
        extensions: [("dev.booharu.test".into(), "kept".into())].into(),
    }
}

#[tokio::test]
async fn word_effects_reset_with_box_effects_in_one_undo_step() {
    let mut session = Session::memory().await.unwrap();
    // A user patch stamps word styles as user-owned, so the fixture starts that way.
    let styled = TextRuns {
        origin: Origin::User,
        runs: vec![
            StyleRun {
                start: 0,
                end: 2,
                style: RunStyle {
                    font: Some("Emphasis".into()),
                    font_weight: Some(700),
                    font_style: Some(FontStyle::Italic),
                    size_scale: Some(1.15),
                    color: Some([255, 0, 0, 255]),
                    stroke_color: Some([255; 4]),
                    stroke_width: Some(2.0),
                    outer_stroke_color: Some([0, 0, 0, 255]),
                    outer_stroke_width: Some(3.0),
                    fill_gradient: Some(gradient()),
                },
            },
            StyleRun {
                start: 2,
                end: 4,
                style: RunStyle {
                    fill_gradient: Some(gradient()),
                    ..RunStyle::default()
                },
            },
        ],
    };
    let effects_only = TextRuns {
        origin: Origin::User,
        runs: vec![styled.runs[1].clone()],
    };
    let fonts_only = strip_word_effects(styled.clone());
    assert!(!word_effects(&fonts_only));
    let originals = [styled, effects_only, fonts_only];
    let mut layers = Vec::new();
    let patch = session
        .snapshot()
        .patch(|edit| {
            let page = edit.add_page(PageDraft::new("page", 100.0, 100.0), At::End)?;
            for (index, runs) in originals.iter().enumerate() {
                let content = edit.add_text_content(page, At::End)?;
                edit.set(
                    content,
                    &koharu_scene::SourceText {
                        text: koharu_scene::Authored {
                            origin: Origin::User,
                            value: "単語".into(),
                        },
                        language: None,
                    },
                )?;
                edit.set(
                    content,
                    &koharu_scene::Translation {
                        text: koharu_scene::Authored {
                            origin: Origin::User,
                            value: "Word".into(),
                        },
                        language: None,
                    },
                )?;
                let layer = edit.add_text_layer(
                    page,
                    At::End,
                    content,
                    &TextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: None,
                        warp: None,
                    },
                )?;
                let mut style = typography();
                if index == 0 {
                    style.outer_stroke_width = Some(8.0);
                }
                edit.set(layer, &style)?;
                edit.set(layer, runs)?;
                layers.push(layer);
            }
            Ok(())
        })
        .unwrap();
    session.commit(patch).await.unwrap();
    let mut project = Project::new(session, "word-effects".into());
    let styles = TextStyles::default();
    let resolver = EffectResolver::new(&project, &styles, false).unwrap();
    let before = project.snapshot();
    assert!(resolver.has_effects(&before, layers[0]).unwrap());
    assert!(resolver.has_effects(&before, layers[1]).unwrap());
    assert!(!resolver.has_effects(&before, layers[2]).unwrap());
    let commit = project
        .reset_effects(
            &[layers[0], layers[1], layers[2], layers[0]],
            &styles,
            false,
        )
        .await
        .unwrap()
        .unwrap();
    project.record_commit(&commit);
    assert_eq!(project.undo.len(), 1);
    let snapshot = project.snapshot();
    assert_eq!(
        snapshot.component::<TextRuns>(layers[0]).unwrap(),
        Some(strip_word_effects(originals[0].clone()))
    );
    assert!(snapshot.component::<TextRuns>(layers[1]).unwrap().is_none());
    assert_eq!(
        snapshot.component::<TextRuns>(layers[2]).unwrap(),
        Some(originals[2].clone())
    );
    let resolver = EffectResolver::new(&project, &styles, false).unwrap();
    for &layer in &layers {
        assert!(!resolver.has_effects(&snapshot, layer).unwrap());
    }
    assert!(
        project
            .reset_effects(&layers, &styles, false)
            .await
            .unwrap()
            .is_none()
    );
    project.undo().await.unwrap();
    for (&layer, runs) in layers.iter().zip(originals) {
        assert_eq!(
            project.snapshot().component::<TextRuns>(layer).unwrap(),
            Some(runs)
        );
        assert_eq!(
            project.snapshot().component::<Typography>(layer).unwrap(),
            before.component::<Typography>(layer).unwrap()
        );
    }
}

fn gradient() -> FillGradient {
    FillGradient {
        kind: GradientKind::Linear,
        angle_degrees: 45.0,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: [0, 0, 0, 255],
            },
            GradientStop {
                offset: 1.0,
                color: [255; 4],
            },
        ],
    }
}

#[test]
fn compares_only_effect_fields_with_width_tolerance() {
    let expected = typography();
    assert!(!effects_differ(&expected, &expected, None));
    let mut changed = expected.clone();
    changed.preferred_font = Some("Other".into());
    changed.color = Some([200, 100, 0, 255]);
    changed.size = Some(40.0);
    changed.font_weight = Some(700);
    changed.font_style = Some(FontStyle::Normal);
    changed.alignment = Some(TextAlignment::Start);
    changed.writing_mode = Some(WritingMode::Horizontal);
    changed.line_spacing = Some(2.0);
    changed.letter_spacing = Some(30.0);
    assert!(!effects_differ(&changed, &expected, None));
    changed.stroke_width = Some(0.009);
    assert!(!effects_differ(&changed, &expected, None));
    changed.stroke_width = Some(0.02);
    assert!(effects_differ(&changed, &expected, None));
    changed = expected.clone();
    changed.outer_stroke_width = Some(3.0);
    assert!(effects_differ(&changed, &expected, None));
    changed = expected.clone();
    changed.fill_gradient = Some(gradient());
    assert!(effects_differ(&changed, &expected, None));
    changed = expected.clone();
    changed.stroke_color = Some([255, 0, 0, 255]);
    assert!(effects_differ(&changed, &expected, None));
    assert!(effects_differ(
        &expected,
        &expected,
        Some(&TextWarp::Skew {
            x_degrees: 5.0,
            y_degrees: 0.0
        })
    ));
}

#[test]
fn automatic_effects_restore_defaults_and_respect_category_switch_and_detected_colors() {
    let mut styles = TextStyles::default();
    styles.default_style.outline = Some(OutlinePart {
        stroke_width: Some(2.0),
        stroke_color: Some([240; 4]),
        stroke_method: Some(StrokeMethod::Contour),
        stroke_position: Some(StrokePosition::Outside),
        stroke_join: Some(StrokeJoin::Round),
    });
    styles.category_styles.sfx.outer_outline = Some(OuterPart {
        outer_stroke_width: Some(4.0),
        outer_stroke_color: Some([0, 0, 0, 255]),
    });
    let mut current = typography();
    current.outer_stroke_width = Some(10.0);
    current.fill_gradient = Some(gradient());
    current.stroke_color = Some([2, 3, 4, 255]);
    let expected = automatic_effects(&current, &styles, Some("sfx"), false);
    assert_eq!(expected.stroke_width, Some(2.0));
    assert_eq!(expected.outer_stroke_width, None);
    assert_eq!(expected.fill_gradient, None);
    styles.shape_styles.enabled = true;
    assert_eq!(
        automatic_effects(&current, &styles, Some("sfx"), false).outer_stroke_width,
        Some(4.0)
    );
    let detected = automatic_effects(&current, &styles, Some("sfx"), true);
    assert_eq!(detected.stroke_color, current.stroke_color);
    assert_eq!(detected.stroke_width, current.stroke_width);
}

#[tokio::test]
async fn reset_preserves_non_effect_fields_and_undo_restores_all_targets_in_one_step() {
    let mut session = Session::memory().await.unwrap();
    let mut original = typography();
    original.outer_stroke_width = Some(8.0);
    original.fill_gradient = Some(gradient());
    let mut layers = Vec::new();
    let patch = session
        .snapshot()
        .patch(|edit| {
            let page = edit.add_page(PageDraft::new("page", 100.0, 100.0), At::End)?;
            for _ in 0..2 {
                let content = edit.add_text_content(page, At::End)?;
                let layer = edit.add_text_layer(
                    page,
                    At::End,
                    content,
                    &TextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: Some(12.0),
                        warp: Some(TextWarp::Skew {
                            x_degrees: 5.0,
                            y_degrees: 0.0,
                        }),
                    },
                )?;
                edit.set(layer, &original)?;
                layers.push(layer);
            }
            Ok(())
        })
        .unwrap();
    session.commit(patch).await.unwrap();
    let mut project = Project::new(session, "effects".into());
    let styles = TextStyles::default();
    let commit = project
        .reset_effects(&layers, &styles, false)
        .await
        .unwrap()
        .unwrap();
    project.record_commit(&commit);
    assert_eq!(project.undo.len(), 1);
    for &layer in &layers {
        let actual = project
            .snapshot()
            .component::<Typography>(layer)
            .unwrap()
            .unwrap();
        let mut expected = original.clone();
        copy_effects(
            &mut expected,
            &automatic_effects(&original, &styles, None, false),
        );
        assert_eq!(actual, expected);
        let layout = project
            .snapshot()
            .component::<TextLayout>(layer)
            .unwrap()
            .unwrap();
        assert_eq!(layout.warp, None);
        assert_eq!(layout.angle_degrees, Some(12.0));
    }
    assert!(
        project
            .reset_effects(&layers, &styles, false)
            .await
            .unwrap()
            .is_none()
    );
    project.undo().await.unwrap();
    for layer in layers {
        assert_eq!(
            project
                .snapshot()
                .component::<Typography>(layer)
                .unwrap()
                .unwrap(),
            original
        );
        assert!(
            project
                .snapshot()
                .component::<TextLayout>(layer)
                .unwrap()
                .unwrap()
                .warp
                .is_some()
        );
    }
}
