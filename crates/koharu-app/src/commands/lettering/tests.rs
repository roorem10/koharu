use super::*;
use booharu_context::{SourceBold, SourceDifference, SourceMeasurements};
use koharu_pipeline::bubble_shape::BubbleShape;
use koharu_renderer::{FontPart, ShapeStyles, TextStyle, TypesettingConfig};
use koharu_scene::{
    At, BubbleRegion, Generation, Geometry, PageDraft, ProducerId, TextLayout, TextLayoutKind,
};

fn label(
    value: LetteringCategory,
    source: LabelSource,
    confidence: Option<f32>,
) -> Option<LineCategory> {
    Some(LineCategory {
        value,
        source,
        confidence,
    })
}

#[test]
fn user_labels_always_win_and_delivery_requires_confidence() {
    use LetteringCategory::*;
    let settings = LetteringSettings::default();
    for category in LetteringCategory::ALL {
        let decision = decide(
            label(category, LabelSource::User, Some(0.0)),
            Some(BubbleShape::Spiky),
            true,
            Some(source_style(SourceBold::Yes, None)),
            &LetteringSettings {
                shout_signals_needed: 1,
                ..settings.clone()
            },
        );
        assert_eq!(decision.category, category);
        assert_eq!(decision.reason, CategoryReason::User);
        assert_eq!(decision.bold, category == Shout);
    }
    for category in [Title, PhoneText, Sfx] {
        let decision = decide(
            label(category, LabelSource::Model, Some(0.0)),
            Some(BubbleShape::Round),
            true,
            None,
            &settings,
        );
        assert_eq!(decision.category, category);
        assert_eq!(decision.reason, CategoryReason::Model);
    }
    for category in [Thoughts, Moan, Whisper, Shy, Crying, Menacing] {
        for confidence in [None, Some(0.6), Some(1.0)] {
            assert_eq!(
                decide(
                    label(category, LabelSource::Model, confidence),
                    Some(BubbleShape::Rectangle),
                    true,
                    None,
                    &settings,
                )
                .category,
                category,
            );
        }
        assert_eq!(
            decide(
                label(category, LabelSource::Model, Some(0.59)),
                Some(BubbleShape::Round),
                true,
                None,
                &settings,
            )
            .category,
            SmallText,
        );
        assert_eq!(
            decide(
                label(category, LabelSource::Model, Some(0.59)),
                Some(BubbleShape::Round),
                false,
                None,
                &settings,
            )
            .category,
            NormalDialogue,
        );
    }
    for category in [NormalDialogue, Floating, SmallText, NarrationBox] {
        assert_eq!(
            decide(
                label(category, LabelSource::Model, Some(1.0)),
                Some(BubbleShape::Spiky),
                false,
                None,
                &settings,
            )
            .category,
            NormalDialogue,
        );
    }
    let strict = LetteringSettings {
        model_confidence: 0.9,
        ..settings
    };
    assert_eq!(
        decide(
            label(Whisper, LabelSource::Model, Some(0.8)),
            None,
            false,
            None,
            &strict,
        )
        .category,
        Floating,
    );
}

fn source_style(bold: SourceBold, difference: Option<SourceDifference>) -> SourceStyle {
    SourceStyle {
        source_bold: bold,
        source_different: bold == SourceBold::Yes || difference.is_some(),
        difference,
        measurements: SourceMeasurements {
            stroke_ratio: 0.15,
            ink_density: 0.3,
            stroke_variation: 0.05,
            glyph_height: 20.0,
            glyph_count: 12,
        },
    }
}

#[test]
fn every_signal_combination_obeys_the_threshold_and_preserves_source_bold() {
    use LetteringCategory::*;
    for required in 1..=3 {
        let settings = LetteringSettings {
            shout_signals_needed: required,
            ..Default::default()
        };
        for yelling_category in [Shout, WeakShout] {
            for bits in 0_u8..8 {
                let bold = bits & 1 != 0;
                let spiky = bits & 2 != 0;
                let yelling = bits & 4 != 0;
                let source = source_style(
                    if bold { SourceBold::Yes } else { SourceBold::No },
                    None,
                );
                let decision = decide(
                    yelling.then_some(LineCategory {
                        value: yelling_category,
                        source: LabelSource::Model,
                        confidence: Some(0.9),
                    }),
                    Some(if spiky {
                        BubbleShape::Spiky
                    } else {
                        BubbleShape::Round
                    }),
                    false,
                    Some(source),
                    &settings,
                );
                let shout = bits.count_ones() >= u32::from(required);
                assert_eq!(decision.category, if shout { Shout } else { NormalDialogue });
                assert_eq!(decision.bold, shout || bold);
                assert_eq!(
                    decision.reason,
                    if shout {
                        CategoryReason::ShoutSignals
                    } else if bold {
                        CategoryReason::SourceStyle
                    } else {
                        CategoryReason::BubbleShape
                    }
                );
                let mut expected = Vec::new();
                if bold {
                    expected.push(ShoutSignal::SourceBold);
                }
                if spiky {
                    expected.push(ShoutSignal::SpikyBubble);
                }
                if yelling {
                    expected.push(ShoutSignal::YellingLine);
                }
                assert_eq!(decision.signals, expected);
                assert_eq!(decision.source_style, Some(source));
            }
        }
    }
    for category in [Shout, WeakShout] {
        let decision = decide(
            label(category, LabelSource::Model, Some(0.59)),
            Some(BubbleShape::Spiky),
            false,
            None,
            &LetteringSettings::default(),
        );
        assert_eq!(decision.category, NormalDialogue);
        assert_eq!(decision.signals, vec![ShoutSignal::SpikyBubble]);
        assert!(!decision.bold);
    }
}

#[test]
fn distinctive_nonbold_original_precedes_model_delivery_but_not_a_hand_label() {
    use LetteringCategory::*;
    let settings = LetteringSettings::default();
    for (difference, category) in [
        (SourceDifference::Thin, Whisper),
        (SourceDifference::Shaky, Crying),
        (SourceDifference::Heavy, Shout),
    ] {
        let source = Some(source_style(SourceBold::No, Some(difference)));
        let automatic = decide(
            label(Menacing, LabelSource::Model, Some(1.0)),
            Some(BubbleShape::Round),
            false,
            source,
            &settings,
        );
        assert_eq!(automatic.category, category);
        assert_eq!(automatic.reason, CategoryReason::SourceStyle);
        assert!(!automatic.bold);
        let manual = decide(
            label(Thoughts, LabelSource::User, None),
            Some(BubbleShape::Spiky),
            false,
            source,
            &settings,
        );
        assert_eq!(manual.category, Thoughts);
        assert_eq!(manual.reason, CategoryReason::User);
    }
    let combined = decide(
        label(WeakShout, LabelSource::Model, Some(0.9)),
        Some(BubbleShape::Spiky),
        false,
        Some(source_style(SourceBold::No, Some(SourceDifference::Heavy))),
        &settings,
    );
    assert_eq!(combined.category, Shout);
    assert_eq!(combined.reason, CategoryReason::ShoutSignals);
    assert!(combined.bold);
    let mut ordinary = source_style(SourceBold::No, Some(SourceDifference::Thin));
    ordinary.source_different = false;
    assert_eq!(
        decide(None, Some(BubbleShape::Round), false, Some(ordinary), &settings).category,
        NormalDialogue,
    );
    assert_eq!(
        decide(
            None,
            Some(BubbleShape::Round),
            false,
            Some(source_style(SourceBold::Unsure, None)),
            &settings,
        )
        .category,
        NormalDialogue,
    );
}

#[test]
fn small_text_precedes_shapes_below_threshold_and_settings_reject_invalid_numbers() {
    use LetteringCategory::*;
    let settings = LetteringSettings::default();
    for (shape, category) in [
        (Some(BubbleShape::Round), NormalDialogue),
        (Some(BubbleShape::Rectangle), NarrationBox),
        (Some(BubbleShape::Spiky), NormalDialogue),
        (None, Floating),
    ] {
        assert_eq!(decide(None, shape, true, None, &settings).category, SmallText);
        assert_eq!(decide(None, shape, false, None, &settings).category, category);
    }
    for required in [0, 4, u8::MAX] {
        assert!(
            LetteringSettings {
                shout_signals_needed: required,
                ..settings.clone()
            }
            .validate()
            .is_err()
        );
    }
    for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert!(
            LetteringSettings {
                model_confidence: value,
                ..settings.clone()
            }
            .validate()
            .is_err()
        );
        assert!(
            LetteringSettings {
                small_text_threshold: value,
                ..settings.clone()
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        LetteringSettings {
            small_text_threshold: 0.0,
            ..settings
        }
        .validate()
        .is_err()
    );
}

async fn project_fixture() -> (tempfile::TempDir, Project, Vec<EntityId>) {
    let directory = tempfile::tempdir().unwrap();
    let mut project = Project::create("lettering".into(), directory.path().join("chapter.khrproj"))
        .await
        .unwrap();
    let generation = Generation::new(ProducerId::new("dev.koharu.pipeline.detection").unwrap());
    let mut edit = project.snapshot().edit_as(generation.clone());
    let page = edit
        .add_page(PageDraft::new("page", 1400.0, 2000.0), At::End)
        .unwrap();
    let bubble = edit
        .add_analysis_region::<BubbleRegion>(
            page,
            At::End,
            &Geometry::rectangle(10.0, 10.0, 200.0, 100.0),
            None,
        )
        .unwrap();
    let mut layers = Vec::new();
    for index in 0..4 {
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
        let decision = decide(
            None,
            (index < 2).then_some(BubbleShape::Rectangle),
            false,
            None,
            &LetteringSettings::default(),
        );
        edit.set(
            layer,
            &Typography {
                preferred_font: Some("original".into()),
                origin: Origin::Generated(generation.clone()),
                font_weight: None,
                font_style: None,
                size: None,
                auto_fit: true,
                color: None,
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
                writing_mode: None,
                extensions: [(
                    DECISION_KEY.to_owned(),
                    serde_json::to_string(&decision).unwrap(),
                )]
                .into(),
            },
        )
        .unwrap();
        if index < 2 {
            edit.relate::<koharu_scene::FlowsIn>(layer, bubble).unwrap();
        }
        layers.push(layer);
    }
    project
        .session
        .commit(edit.finish().unwrap())
        .await
        .unwrap();
    let snapshot = project.snapshot();
    let patch = snapshot
        .patch(|edit| {
            let mut manual = snapshot.component::<Typography>(layers[1])?.unwrap();
            manual.origin = Origin::User;
            manual.preferred_font = Some("manual".into());
            edit.set(layers[1], &manual)
        })
        .unwrap();
    project.session.commit(patch).await.unwrap();
    (directory, project, layers)
}

fn font(name: &str) -> TextStyle {
    TextStyle {
        font: Some(FontPart {
            preferred_font: Some(name.into()),
            font_weight: None,
            font_style: None,
        }),
        ..Default::default()
    }
}

fn current_font(project: &Project, layer: EntityId) -> Option<String> {
    project
        .snapshot()
        .component::<Typography>(layer)
        .unwrap()
        .unwrap()
        .preferred_font
}

#[tokio::test]
async fn source_weight_precedes_category_defaults_and_preserves_hand_choices() {
    let (_directory, project, layers) = project_fixture().await;
    let mut typography = project
        .snapshot()
        .component::<Typography>(layers[0])
        .unwrap()
        .unwrap();
    let settings = LetteringSettings::default();
    for (source_bold, expected) in [(SourceBold::No, 400), (SourceBold::Yes, 800)] {
        typography.font_weight = Some(800);
        let decision = decide(
            None,
            Some(BubbleShape::Round),
            false,
            Some(source_style(source_bold, None)),
            &settings,
        );
        apply_source_weight(&mut typography, &decision);
        assert_eq!(typography.font_weight, Some(expected));
    }
    typography.font_weight = Some(400);
    let bold = decide(
        None,
        Some(BubbleShape::Round),
        false,
        Some(source_style(SourceBold::Yes, None)),
        &settings,
    );
    apply_source_weight(&mut typography, &bold);
    assert_eq!(typography.font_weight, Some(700));
    let manual = decide(
        label(LetteringCategory::Thoughts, LabelSource::User, None),
        Some(BubbleShape::Round),
        false,
        Some(source_style(SourceBold::No, None)),
        &settings,
    );
    apply_source_weight(&mut typography, &manual);
    assert_eq!(typography.font_weight, Some(700));
}

#[tokio::test]
async fn normal_category_clears_a_previous_shout_font_when_presets_are_empty() {
    let (_directory, project, layers) = project_fixture().await;
    let renderer = Renderer::from_config(koharu_config::Config::memory(
        TypesettingConfig::default(),
    ))
    .unwrap();
    let mut typography = project
        .snapshot()
        .component::<Typography>(layers[0])
        .unwrap()
        .unwrap();
    let origin = typography.origin.clone();
    typography.preferred_font = Some("old shout".into());
    typography.font_weight = Some(700);
    typography.font_style = Some(koharu_scene::FontStyle::Italic);
    typography.color = Some([12, 34, 56, 255]);
    typography.stroke_color = Some([240, 230, 220, 255]);
    typography.stroke_width = Some(2.0);
    typography.writing_mode = Some(koharu_scene::WritingMode::Vertical);
    let styles = TextStyles {
        shape_styles: ShapeStyles {
            enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let decision = decide(
        None,
        Some(BubbleShape::Spiky),
        false,
        None,
        &LetteringSettings::default(),
    );
    assert_eq!(decision.category, LetteringCategory::NormalDialogue);
    assert!(!decision.bold);
    let prepared = prepare_typography(
        &styles,
        &decision,
        typography,
        true,
        &renderer,
        None,
    )
    .await
    .unwrap();
    assert_eq!(prepared.preferred_font, None);
    assert_eq!(prepared.font_weight, None);
    assert_eq!(prepared.font_style, None);
    assert_eq!(prepared.color, Some([12, 34, 56, 255]));
    assert_eq!(prepared.stroke_color, Some([240, 230, 220, 255]));
    assert_eq!(prepared.stroke_width, Some(2.0));
    assert_eq!(prepared.writing_mode, Some(koharu_scene::WritingMode::Vertical));
    assert_eq!(prepared.origin, origin);
}

#[tokio::test]
async fn label_changes_restyle_only_requested_layers_in_one_undo() {
    let (_directory, mut project, layers) = project_fixture().await;
    let renderer = Renderer::from_config(koharu_config::Config::memory(
        TypesettingConfig::default(),
    ))
    .unwrap();
    let original = project
        .snapshot()
        .component::<Typography>(layers[0])
        .unwrap()
        .unwrap();
    let manual = project
        .snapshot()
        .component::<Typography>(layers[1])
        .unwrap()
        .unwrap();
    let mut styles = TextStyles {
        shape_styles: ShapeStyles {
            enabled: true,
            rectangle: font("narration"),
            ..Default::default()
        },
        ..Default::default()
    };
    styles.category_styles.whisper = font("whisper");
    styles.category_styles.thoughts = font("thoughts");
    let settings = LetteringSettings::default();
    project
        .update_context(|context| {
            for &layer in &layers[..2] {
                context.set_model_category(layer, LetteringCategory::Whisper, Some(0.9));
            }
        })
        .unwrap();
    let commit = restyle_labels(
        &mut project,
        &layers[..2],
        &styles,
        &settings,
        false,
        false,
        &renderer,
    )
    .await
    .unwrap()
    .unwrap();
    project.record_commit(&commit);
    drop(commit);
    assert_eq!(project.undo.len(), 1);
    assert_eq!(
        current_font(&project, layers[0]).as_deref(),
        Some("whisper")
    );
    assert_eq!(current_font(&project, layers[1]).as_deref(), Some("manual"));
    assert_eq!(
        current_font(&project, layers[2]).as_deref(),
        Some("original")
    );
    let applied = project
        .snapshot()
        .component::<Typography>(layers[0])
        .unwrap()
        .unwrap();
    let decision: EffectiveCategory =
        serde_json::from_str(applied.extensions.get(DECISION_KEY).unwrap()).unwrap();
    assert_eq!(decision.category, LetteringCategory::Whisper);
    assert_eq!(decision.reason, CategoryReason::Model);
    assert!(
        restyle_labels(
            &mut project,
            &layers[..2],
            &styles,
            &settings,
            false,
            false,
            &renderer,
        )
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(project.undo.len(), 1);
    project.undo().await.unwrap();
    assert_eq!(
        project
            .snapshot()
            .component::<Typography>(layers[0])
            .unwrap()
            .unwrap(),
        original,
    );
    assert!(project.undo.is_empty());

    project
        .update_context(|context| context.set_user_category(layers[1], None))
        .unwrap();
    for _ in 0..2 {
        assert!(
            restyle_labels(
                &mut project,
                &[layers[1]],
                &styles,
                &settings,
                false,
                false,
                &renderer,
            )
            .await
            .unwrap()
            .is_none()
        );
        assert_eq!(
            project
                .snapshot()
                .component::<Typography>(layers[1])
                .unwrap()
                .unwrap(),
            manual,
        );
    }

    project
        .update_context(|context| {
            context.set_user_category(layers[1], Some(LetteringCategory::Thoughts))
        })
        .unwrap();
    restyle_labels(
        &mut project,
        &[layers[1]],
        &styles,
        &settings,
        false,
        true,
        &renderer,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        current_font(&project, layers[1]).as_deref(),
        Some("thoughts")
    );
    assert_eq!(
        project
            .snapshot()
            .component::<Typography>(layers[1])
            .unwrap()
            .unwrap()
            .origin,
        Origin::User
    );
    let path = project.directory().unwrap().to_owned();
    drop(project);
    let mut project = Project::open("lettering".into(), path).await.unwrap();
    assert_eq!(
        project
            .load_context()
            .unwrap()
            .category(layers[1])
            .unwrap()
            .source,
        LabelSource::User
    );
    project
        .update_context(|context| context.set_user_category(layers[1], None))
        .unwrap();
    restyle_labels(
        &mut project,
        &[layers[1]],
        &styles,
        &settings,
        false,
        true,
        &renderer,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        current_font(&project, layers[1]).as_deref(),
        Some("narration")
    );
    styles.shape_styles.enabled = false;
    assert!(
        restyle_labels(
            &mut project,
            &layers,
            &styles,
            &settings,
            false,
            false,
            &renderer,
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn sfx_modes_filter_effective_categories_and_bubble_membership() {
    let (_directory, project, layers) = project_fixture().await;
    let mut context = ProjectContext::default();
    context.set_model_category(layers[0], LetteringCategory::Sfx, Some(0.0));
    context.set_user_category(layers[2], Some(LetteringCategory::Sfx));
    context.set_user_category(layers[3], Some(LetteringCategory::NormalDialogue));
    let mut settings = LetteringSettings::default();
    assert_eq!(
        translation_skip_with(&context, &project.snapshot(), &settings).unwrap(),
        [layers[0], layers[2]].into()
    );
    settings.sfx = SfxMode::InsideBubbles;
    assert_eq!(
        translation_skip_with(&context, &project.snapshot(), &settings).unwrap(),
        [layers[2]].into()
    );
    settings.sfx = SfxMode::TranslateAll;
    assert!(
        translation_skip_with(&context, &project.snapshot(), &settings)
            .unwrap()
            .is_empty()
    );
    context.set_user_category(layers[0], Some(LetteringCategory::Whisper));
    settings.sfx = SfxMode::Skip;
    assert_eq!(
        translation_skip_with(&context, &project.snapshot(), &settings).unwrap(),
        [layers[2]].into()
    );
    let effective = resolve_category(
        &context,
        &project.snapshot(),
        layers[0],
        &text_bubble_shape(&project.snapshot(), layers[0]).unwrap(),
        &settings,
    )
    .unwrap();
    assert_eq!(effective.reason, CategoryReason::User);
}
