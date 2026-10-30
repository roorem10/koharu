use super::*;
use koharu_renderer::{FontPart, ShapeStyles};
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
        assert_eq!(
            effective_category(
                label(category, LabelSource::User, Some(0.0)),
                Some(BubbleShape::Spiky),
                true,
                &settings
            ),
            category
        );
    }
    for category in [Title, PhoneText, Sfx] {
        assert_eq!(
            effective_category(
                label(category, LabelSource::Model, Some(0.0)),
                Some(BubbleShape::Round),
                true,
                &settings
            ),
            category
        );
    }
    for category in [
        Thoughts, WeakShout, Moan, Whisper, Shy, Crying, Menacing, Shout,
    ] {
        for confidence in [None, Some(0.6), Some(1.0)] {
            assert_eq!(
                effective_category(
                    label(category, LabelSource::Model, confidence),
                    Some(BubbleShape::Rectangle),
                    true,
                    &settings
                ),
                category
            );
        }
        assert_eq!(
            effective_category(
                label(category, LabelSource::Model, Some(0.59)),
                Some(BubbleShape::Round),
                true,
                &settings
            ),
            SmallText
        );
        assert_eq!(
            effective_category(
                label(category, LabelSource::Model, Some(0.59)),
                Some(BubbleShape::Round),
                false,
                &settings
            ),
            NormalDialogue
        );
    }
    for category in [NormalDialogue, Floating, SmallText, NarrationBox] {
        assert_eq!(
            effective_category(
                label(category, LabelSource::Model, Some(1.0)),
                Some(BubbleShape::Spiky),
                false,
                &settings
            ),
            Shout
        );
    }
    let strict = LetteringSettings {
        model_confidence: 0.9,
        ..settings
    };
    assert_eq!(
        effective_category(
            label(Whisper, LabelSource::Model, Some(0.8)),
            None,
            false,
            &strict
        ),
        Floating
    );
}

#[test]
fn small_text_precedes_every_shape_and_settings_reject_invalid_numbers() {
    use LetteringCategory::*;
    let settings = LetteringSettings::default();
    for (shape, category) in [
        (Some(BubbleShape::Round), NormalDialogue),
        (Some(BubbleShape::Rectangle), NarrationBox),
        (Some(BubbleShape::Spiky), Shout),
        (None, Floating),
    ] {
        assert_eq!(effective_category(None, shape, true, &settings), SmallText);
        assert_eq!(effective_category(None, shape, false, &settings), category);
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
                },
            )
            .unwrap();
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
                alignment: None,
                writing_mode: None,
                extensions: Default::default(),
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
async fn label_changes_restyle_only_requested_layers_in_one_undo() {
    let (_directory, mut project, layers) = project_fixture().await;
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
    let commit = restyle_labels(&mut project, &layers[..2], &styles, &settings, false)
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
    project.undo().await.unwrap();
    assert_eq!(
        current_font(&project, layers[0]).as_deref(),
        Some("original")
    );

    project
        .update_context(|context| {
            context.set_user_category(layers[1], Some(LetteringCategory::Thoughts))
        })
        .unwrap();
    restyle_labels(&mut project, &[layers[1]], &styles, &settings, false)
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
    restyle_labels(&mut project, &[layers[1]], &styles, &settings, false)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        current_font(&project, layers[1]).as_deref(),
        Some("narration")
    );
    styles.shape_styles.enabled = false;
    assert!(
        restyle_labels(&mut project, &layers, &styles, &settings, false)
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
