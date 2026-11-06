use super::*;
use koharu_renderer::{FontPart, OutlinePart, SizePart};
use koharu_scene::{
    At, EntityOrigin, FillGradient, FontStyle, GradientKind, GradientStop, LanguageTag, PageDraft,
    Session, TextAlignment, TextLayout, TextLayoutKind, TextWarp, WritingMode,
};

fn generation() -> Generation {
    Generation::new(ProducerId::new("dev.koharu.pipeline.translation").unwrap())
}

fn renderer() -> Renderer {
    Renderer::from_config(koharu_config::Config::memory(
        koharu_renderer::TypesettingConfig {
            font_families: Vec::new(),
        },
    ))
    .unwrap()
}

fn bold() -> RunStyle {
    RunStyle {
        font_weight: Some(700),
        ..Default::default()
    }
}

fn style_patch(style: RunStyle) -> RunStylePatch {
    RunStylePatch {
        set: style,
        clear: vec![],
    }
}

fn runs(start: u32, end: u32) -> TextRuns {
    TextRuns {
        origin: Origin::User,
        runs: vec![StyleRun {
            start,
            end,
            style: bold(),
        }],
    }
}

fn translation(text: &str) -> Translation {
    Translation {
        text: Authored::generated(text.into(), generation()),
        language: Some(LanguageTag::new("en").unwrap()),
    }
}

async fn fixture(translated: bool) -> (Project, [EntityId; 2]) {
    let mut session = Session::memory().await.unwrap();
    let mut edit = session.snapshot().edit_as(generation());
    let page = edit
        .add_page(PageDraft::new("page", 400.0, 600.0), At::End)
        .unwrap();
    let mut layers = Vec::new();
    for _ in 0..2 {
        let content = edit.add_text_content(page, At::End).unwrap();
        edit.set(
            content,
            &SourceText {
                text: Authored::generated("hello magic!".into(), generation()),
                language: Some(LanguageTag::new("ja").unwrap()),
            },
        )
        .unwrap();
        let translation = translation("hello magic!");
        if translated {
            edit.set(content, &translation).unwrap();
        }
        let layer = edit
            .add_text_layer(
                page,
                At::End,
                content,
                &TextLayout {
                    origin: Origin::Generated(generation()),
                    kind: TextLayoutKind::Paragraph,
                    angle_degrees: None,
                    warp: None,
                },
            )
            .unwrap();
        edit.set(layer, &default_typography(None, &translation, false))
            .unwrap();
        layers.push(layer);
    }
    session.commit(edit.finish().unwrap()).await.unwrap();
    (
        Project::new(session, "rich text".into()),
        layers.try_into().unwrap(),
    )
}

async fn machine_translation(project: &mut Project, layers: &[EntityId]) -> Commit {
    let snapshot = project.snapshot();
    let mut edit = snapshot.edit_as(generation());
    for &layer in layers {
        let content = snapshot.text_layer(layer).unwrap().content().unwrap().id();
        edit.set(content, &translation("new words")).unwrap();
    }
    project.commit(edit.finish().unwrap()).await.unwrap()
}

#[tokio::test]
async fn setting_word_styles_is_one_undo_step_and_redo_restores_them() {
    let (mut project, [layer, _]) = fixture(true).await;
    let before = project.snapshot();
    let commit = project
        .set_word_style(layer, 6, 11, style_patch(bold()))
        .await
        .unwrap()
        .unwrap();
    project.record_commit(&commit);
    assert_eq!(project.undo.len(), 1);
    assert_eq!(project.word_styles(layer).unwrap(), runs(6, 11).runs);
    assert_eq!(
        project
            .snapshot()
            .component::<EntityOrigin>(layer)
            .unwrap()
            .unwrap()
            .origin,
        Origin::User
    );
    assert!(
        project
            .set_word_style(layer, 6, 11, style_patch(bold()))
            .await
            .unwrap()
            .is_none()
    );
    project.undo().await.unwrap();
    assert!(project.word_styles(layer).unwrap().is_empty());
    assert_eq!(
        project.snapshot().component::<EntityOrigin>(layer).unwrap(),
        before.component::<EntityOrigin>(layer).unwrap()
    );
    assert!(project.undo.is_empty());
    project.redo().await.unwrap();
    assert_eq!(project.word_styles(layer).unwrap(), runs(6, 11).runs);
}

#[tokio::test]
async fn clearing_multiple_boxes_deduplicates_and_is_one_undo_step() {
    let (mut project, layers) = fixture(true).await;
    for layer in layers {
        project
            .set_word_style(layer, 6, 11, style_patch(bold()))
            .await
            .unwrap();
    }
    let commit = project
        .clear_word_styles(&[layers[0], layers[1], layers[0]], Some([8, 99]))
        .await
        .unwrap()
        .unwrap();
    project.record_commit(&commit);
    for layer in layers {
        assert_eq!(project.word_styles(layer).unwrap(), runs(6, 8).runs);
    }
    assert_eq!(project.undo.len(), 1);
    project.undo().await.unwrap();
    for layer in layers {
        assert_eq!(project.word_styles(layer).unwrap(), runs(6, 11).runs);
    }
    let commit = project
        .clear_word_styles(&layers, None)
        .await
        .unwrap()
        .unwrap();
    project.record_commit(&commit);
    for layer in layers {
        assert!(
            project
                .snapshot()
                .component::<TextRuns>(layer)
                .unwrap()
                .is_none()
        );
    }
    let revision = project.revision();
    assert!(
        project
            .clear_word_styles(&layers, None)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(project.revision(), revision);
    project.undo().await.unwrap();
    for layer in layers {
        assert_eq!(project.word_styles(layer).unwrap(), runs(6, 11).runs);
    }
}

#[tokio::test]
async fn clearing_the_last_field_removes_the_component_and_noops_do_not_promote_layers() {
    let (mut project, [layer, other]) = fixture(true).await;
    let before = project.snapshot();
    assert!(
        project
            .set_word_style(other, 0, 1, RunStylePatch::default())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        project
            .clear_word_styles(&[other], Some([0, 1]))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        project.snapshot().component::<EntityOrigin>(other).unwrap(),
        before.component::<EntityOrigin>(other).unwrap()
    );
    project
        .set_word_style(layer, 0, 1, style_patch(bold()))
        .await
        .unwrap();
    let commit = project
        .set_word_style(
            layer,
            0,
            1,
            RunStylePatch {
                set: RunStyle::default(),
                clear: vec![RunStyleField::FontWeight],
            },
        )
        .await
        .unwrap()
        .unwrap();
    project.record_commit(&commit);
    assert!(
        project
            .snapshot()
            .component::<TextRuns>(layer)
            .unwrap()
            .is_none()
    );
    project.undo().await.unwrap();
    assert_eq!(project.word_styles(layer).unwrap(), runs(0, 1).runs);
}

#[tokio::test]
async fn replacing_unicode_text_keeps_styles_and_text_in_one_undo_step() {
    for translated in [false, true] {
        let (mut project, [layer, _]) = fixture(translated).await;
        if translated {
            project
                .set_translation(layer, Some("aé👻magic!".into()))
                .await
                .unwrap();
        } else {
            project
                .set_source_text(layer, "aé👻magic!".into())
                .await
                .unwrap();
        }
        project
            .set_word_style(layer, 3, 8, style_patch(bold()))
            .await
            .unwrap();
        let commit = project
            .replace_text_range(layer, 4, 4, "王".into())
            .await
            .unwrap()
            .unwrap();
        project.record_commit(&commit);
        assert_eq!(
            shown_text(&project.snapshot(), layer).unwrap(),
            "aé👻m王agic!"
        );
        assert_eq!(project.word_styles(layer).unwrap(), runs(3, 9).runs);
        let snapshot = project.snapshot();
        let content = snapshot.text_layer(layer).unwrap().content().unwrap();
        if translated {
            let value = content.translation().unwrap().unwrap();
            assert_eq!(value.text.origin, Origin::User);
            assert_eq!(value.language.unwrap().as_str(), "en");
        } else {
            assert!(content.translation().unwrap().is_none());
            let value = content.source().unwrap().unwrap();
            assert_eq!(value.text.origin, Origin::User);
            assert_eq!(value.language.unwrap().as_str(), "ja");
        }
        assert_eq!(project.undo.len(), 1);
        project.undo().await.unwrap();
        assert_eq!(
            shown_text(&project.snapshot(), layer).unwrap(),
            "aé👻magic!"
        );
        assert_eq!(project.word_styles(layer).unwrap(), runs(3, 8).runs);
        assert!(project.undo.is_empty());
        project.redo().await.unwrap();
        assert_eq!(project.word_styles(layer).unwrap(), runs(3, 9).runs);
    }
}

#[tokio::test]
async fn known_range_replacement_does_not_guess_an_insertion_among_repeated_letters() {
    let (mut project, [layer, _]) = fixture(true).await;
    project
        .set_translation(layer, Some("aa".into()))
        .await
        .unwrap();
    project
        .set_word_style(layer, 0, 1, style_patch(bold()))
        .await
        .unwrap();
    project
        .replace_text_range(layer, 1, 1, "a".into())
        .await
        .unwrap();
    assert_eq!(shown_text(&project.snapshot(), layer).unwrap(), "aaa");
    assert_eq!(project.word_styles(layer).unwrap(), runs(0, 2).runs);
}

#[tokio::test]
async fn range_and_style_errors_leave_the_project_unchanged() {
    let (mut project, [layer, _]) = fixture(true).await;
    let revision = project.revision();
    for (start, end) in [(0, 0), (3, 2), (0, 13), (u32::MAX, u32::MAX)] {
        assert!(
            project
                .set_word_style(layer, start, end, style_patch(bold()))
                .await
                .is_err()
        );
    }
    for (start, end) in [(3, 2), (0, 13), (u32::MAX, u32::MAX)] {
        assert!(
            project
                .replace_text_range(layer, start, end, "x".into())
                .await
                .is_err()
        );
    }
    assert!(
        project
            .clear_word_styles(&[layer], Some([3, 2]))
            .await
            .is_err()
    );
    assert!(
        project
            .set_word_style(
                layer,
                0,
                2,
                style_patch(RunStyle {
                    size_scale: Some(8.0),
                    ..Default::default()
                })
            )
            .await
            .is_err()
    );
    assert!(
        project
            .replace_text_range(layer, 0, 0, "\0".into())
            .await
            .is_err()
    );
    assert!(
        project
            .replace_text_range(layer, 0, 0, String::new())
            .await
            .unwrap()
            .is_none()
    );
    let page = project.active_page().unwrap();
    assert!(
        project
            .set_word_style(page, 0, 1, style_patch(bold()))
            .await
            .is_err()
    );
    assert!(
        project
            .clear_word_styles(&[layer, page], None)
            .await
            .is_err()
    );
    assert!(project.word_styles(page).is_err());
    assert_eq!(project.revision(), revision);
    assert!(project.undo.is_empty());
}

#[tokio::test]
async fn stored_ranges_are_clamped_on_reads_and_writes() {
    let (mut project, [layer, _]) = fixture(true).await;
    let patch = project
        .snapshot()
        .patch(|edit| edit.set(layer, &runs(6, 99)))
        .unwrap();
    project.commit(patch).await.unwrap();
    assert_eq!(project.word_styles(layer).unwrap(), runs(6, 12).runs);
    project
        .set_word_style(layer, 6, 7, style_patch(bold()))
        .await
        .unwrap();
    let stored = project
        .snapshot()
        .component::<TextRuns>(layer)
        .unwrap()
        .unwrap();
    assert_eq!(stored.runs.last().unwrap().end, 12);
}

#[tokio::test]
async fn panel_edits_remap_styles_but_hidden_source_edits_do_not() {
    let (mut project, [layer, _]) = fixture(true).await;
    project
        .set_word_style(layer, 6, 11, style_patch(bold()))
        .await
        .unwrap();
    let commit = project
        .set_translation(layer, Some("prefix hello magic!".into()))
        .await
        .unwrap();
    project.record_commit(&commit);
    assert_eq!(project.word_styles(layer).unwrap(), runs(13, 18).runs);
    assert_eq!(project.undo.len(), 1);
    project.undo().await.unwrap();
    assert_eq!(project.word_styles(layer).unwrap(), runs(6, 11).runs);
    project
        .set_source_text(layer, "different source".into())
        .await
        .unwrap();
    assert_eq!(project.word_styles(layer).unwrap(), runs(6, 11).runs);
    let (mut source_only, [source_layer, _]) = fixture(false).await;
    source_only
        .set_word_style(source_layer, 6, 11, style_patch(bold()))
        .await
        .unwrap();
    source_only
        .set_source_text(source_layer, "X hello magic!".into())
        .await
        .unwrap();
    assert_eq!(
        source_only.word_styles(source_layer).unwrap(),
        runs(8, 13).runs
    );
}

#[tokio::test]
async fn shared_presentations_remap_and_translation_reset_targets_each_presentation() {
    let (mut project, [layer, _]) = fixture(true).await;
    let snapshot = project.snapshot();
    let content = snapshot.text_layer(layer).unwrap().content().unwrap().id();
    let page = project.active_page().unwrap();
    let mut sibling = None;
    let patch = snapshot
        .patch(|edit| {
            let id = edit.add_text_layer(
                page,
                At::End,
                content,
                &snapshot.text_layer(layer)?.layout()?,
            )?;
            edit.set(id, &snapshot.component::<Typography>(layer)?.unwrap())?;
            sibling = Some(id);
            Ok(())
        })
        .unwrap();
    project.commit(patch).await.unwrap();
    let sibling = sibling.unwrap();
    for id in [layer, sibling] {
        project
            .set_word_style(id, 6, 11, style_patch(bold()))
            .await
            .unwrap();
    }
    project
        .set_translation(layer, Some("X hello magic!".into()))
        .await
        .unwrap();
    for id in [layer, sibling] {
        assert_eq!(project.word_styles(id).unwrap(), runs(8, 13).runs);
    }
    project
        .replace_text_range(layer, 0, 2, String::new())
        .await
        .unwrap();
    for id in [layer, sibling] {
        assert_eq!(project.word_styles(id).unwrap(), runs(6, 11).runs);
    }
    project.set_translation(layer, None).await.unwrap();
    let stage = machine_translation(&mut project, &[layer]).await;
    let mut expected = vec![layer, sibling];
    expected.sort();
    assert_eq!(
        translated_layers(&project.snapshot(), &stage).unwrap(),
        expected
    );
}

fn changed_typography() -> Typography {
    let mut value = default_typography(None, &translation("old"), false);
    value.origin = Origin::User;
    value.preferred_font = Some("Hand edited".into());
    value.font_weight = Some(900);
    value.font_style = Some(FontStyle::Italic);
    value.auto_fit = false;
    value.size = Some(72.0);
    value.color = Some([50, 60, 70, 255]);
    value.stroke_color = Some([80, 90, 100, 255]);
    value.stroke_width = Some(12.0);
    value.outer_stroke_color = Some([255; 4]);
    value.outer_stroke_width = Some(5.0);
    value.line_spacing = Some(2.0);
    value.letter_spacing = Some(100.0);
    value.fill_gradient = Some(FillGradient {
        kind: GradientKind::Linear,
        angle_degrees: 30.0,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: [0; 4],
            },
            GradientStop {
                offset: 1.0,
                color: [255; 4],
            },
        ],
    });
    value.alignment = Some(TextAlignment::End);
    value.writing_mode = Some(WritingMode::Vertical);
    value
        .extensions
        .insert("dev.booharu.test".into(), "clear".into());
    value
}

#[tokio::test]
async fn translation_reset_preserves_hand_typography_runs_and_warp() {
    let (mut project, [layer, skipped]) = fixture(true).await;
    let original = changed_typography();
    let patch = project
        .snapshot()
        .patch(|edit| {
            for id in [layer, skipped] {
                edit.set(id, &runs(6, 11))?;
                edit.set(id, &original)?;
                edit.set(
                    id,
                    &TextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: Some(15.0),
                        warp: Some(TextWarp::Skew {
                            x_degrees: 10.0,
                            y_degrees: 2.0,
                        }),
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    project.commit(patch).await.unwrap();
    let before = project.snapshot();
    let stage = machine_translation(&mut project, &[layer]).await;
    let mut hinted = Vec::new();
    let reset = reset_translation_styles(
        &mut project,
        &stage,
        &TextStyles::default(),
        false,
        &renderer(),
        |_, _, layers, _| {
            hinted = layers.to_vec();
            Ok(())
        },
    )
    .await
    .unwrap();
    assert!(reset.is_none());
    assert!(hinted.is_empty());
    for id in [layer, skipped] {
        assert_eq!(
            project.snapshot().component::<TextRuns>(id).unwrap(),
            before.component::<TextRuns>(id).unwrap()
        );
        assert_eq!(
            project.snapshot().component::<Typography>(id).unwrap(),
            before.component::<Typography>(id).unwrap()
        );
        assert_eq!(
            project.snapshot().component::<TextLayout>(id).unwrap(),
            before.component::<TextLayout>(id).unwrap()
        );
    }
    assert_eq!(shown_text(&project.snapshot(), layer).unwrap(), "new words");
    assert!(project.undo.is_empty());
}

#[tokio::test]
async fn new_translation_hints_can_write_runs_in_the_same_reset_commit() {
    let (mut project, [layer, _]) = fixture(true).await;
    let generated = generation();
    let old_runs = TextRuns {
        origin: Origin::Generated(generated.clone()),
        ..runs(6, 11)
    };
    let old_layout = TextLayout {
        origin: Origin::Generated(generated.clone()),
        kind: TextLayoutKind::Paragraph,
        angle_degrees: Some(15.0),
        warp: Some(TextWarp::Skew {
            x_degrees: 10.0,
            y_degrees: 2.0,
        }),
    };
    let mut edit = project.snapshot().edit_as(generated);
    edit.set(layer, &old_runs).unwrap();
    edit.set(layer, &old_layout).unwrap();
    project.commit(edit.finish().unwrap()).await.unwrap();
    let stage = machine_translation(&mut project, &[layer]).await;
    let commit = reset_translation_styles(
        &mut project,
        &stage,
        &TextStyles::default(),
        false,
        &renderer(),
        |edit, snapshot, layers, _| {
            assert_eq!(layers, &[layer]);
            assert_eq!(shown_text(snapshot, layer)?, "new words");
            edit.set(layer, &runs(0, 3))
        },
    )
    .await
    .unwrap()
    .unwrap();
    project.record_commit(&commit);
    assert_eq!(project.undo.len(), 1);
    assert_eq!(project.word_styles(layer).unwrap(), runs(0, 3).runs);
    let layout = project.snapshot().text_layer(layer).unwrap().layout().unwrap();
    assert!(layout.warp.is_none());
    assert_eq!(layout.angle_degrees, old_layout.angle_degrees);
    project.undo().await.unwrap();
    assert_eq!(
        project.snapshot().component::<TextRuns>(layer).unwrap(),
        Some(old_runs)
    );
    assert_eq!(project.snapshot().component::<TextLayout>(layer).unwrap(), Some(old_layout));
    assert_eq!(project.word_styles(layer).unwrap(), runs(6, 9).runs);
}

#[tokio::test]
async fn unchanged_plain_box_adds_no_reset_commit_but_still_receives_the_hints_hook() {
    let (mut project, [layer, _]) = fixture(true).await;
    let stage = machine_translation(&mut project, &[layer]).await;
    let revision = project.revision();
    let mut hinted = Vec::new();
    let result = reset_translation_styles(
        &mut project,
        &stage,
        &TextStyles::default(),
        false,
        &renderer(),
        |_, _, layers, _| {
            hinted = layers.to_vec();
            Ok(())
        },
    )
    .await
    .unwrap();
    assert!(result.is_none());
    assert_eq!(hinted, vec![layer]);
    assert_eq!(project.revision(), revision);
    let unrelated = project
        .set_source_text(layer, "source edit".into())
        .await
        .unwrap();
    assert!(
        translated_layers(&project.snapshot(), &unrelated)
            .unwrap()
            .is_empty()
    );
    assert!(
        after_translation(&mut project, &unrelated, &renderer())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn default_reset_preserves_detected_paint_and_hand_runs_and_warp() {
    let (mut project, [layer, _]) = fixture(true).await;
    let generated = generation();
    let mut original = changed_typography();
    original.origin = Origin::Generated(generated.clone());
    let mut edit = project.snapshot().edit_as(generated);
    edit.set(layer, &original).unwrap();
    project.commit(edit.finish().unwrap()).await.unwrap();
    let layout = TextLayout {
        origin: Origin::User,
        kind: TextLayoutKind::Paragraph,
        angle_degrees: Some(15.0),
        warp: Some(TextWarp::Skew {
            x_degrees: 10.0,
            y_degrees: 2.0,
        }),
    };
    let patch = project
        .snapshot()
        .patch(|edit| {
            edit.set(layer, &runs(0, 3))?;
            edit.set(layer, &layout)
        })
        .unwrap();
    project.commit(patch).await.unwrap();
    let mut styles = TextStyles::default();
    styles.default_style.fill = Some([200; 4]);
    styles.default_style.outline = Some(OutlinePart {
        stroke_width: Some(1.0),
        stroke_color: Some([255; 4]),
        stroke_method: Some(StrokeMethod::Contour),
        stroke_position: None,
        stroke_join: None,
    });
    let stage = machine_translation(&mut project, &[layer]).await;
    let reset = reset_translation_styles(
        &mut project,
        &stage,
        &styles,
        true,
        &renderer(),
        |_, _, layers, _| {
            assert!(layers.is_empty());
            Ok(())
        },
    )
    .await
    .unwrap()
    .unwrap();
    project.record_commit(&reset);
    let actual = project
        .snapshot()
        .component::<Typography>(layer)
        .unwrap()
        .unwrap();
    assert_eq!(actual.color, original.color);
    assert_eq!(actual.stroke_color, original.stroke_color);
    assert_eq!(actual.stroke_width, original.stroke_width);
    assert!(actual.fill_gradient.is_none());
    assert!(actual.size.is_none());
    assert!(actual.auto_fit);
    assert!(matches!(actual.origin, Origin::Generated(_)));
    assert_eq!(
        project.snapshot().component::<TextRuns>(layer).unwrap(),
        Some(runs(0, 3))
    );
    assert_eq!(
        project.snapshot().component::<TextLayout>(layer).unwrap(),
        Some(layout)
    );
    project.undo().await.unwrap();
    assert_eq!(
        project.snapshot().component::<Typography>(layer).unwrap(),
        Some(original)
    );
}

#[tokio::test]
async fn translation_reset_uses_the_lettering_category_default() {
    let (mut project, [layer, _]) = fixture(true).await;
    let directory = tempfile::tempdir().unwrap();
    project.directory = Some(directory.path().into());
    project
        .update_context(|context| {
            context.set_user_category(layer, Some(booharu_context::LetteringCategory::Whisper));
        })
        .unwrap();
    let mut styles = TextStyles::default();
    styles.shape_styles.enabled = true;
    let renderer = renderer();
    let family = renderer
        .covering_font(
            &[FontPart {
                preferred_font: None,
                font_weight: Some(500),
                font_style: Some(FontStyle::Italic),
            }],
            "new words",
            Some("en"),
        )
        .await
        .unwrap()
        .expect("an installed text font must cover the Latin fixture");
    styles.default_style.font = Some(FontPart {
        preferred_font: Some(family.clone()),
        font_weight: Some(400),
        font_style: None,
    });
    styles.category_styles.whisper.font = Some(FontPart {
        preferred_font: Some(family.clone()),
        font_weight: Some(500),
        font_style: Some(FontStyle::Italic),
    });
    styles.category_styles.whisper.size = Some(SizePart {
        auto_fit: false,
        size: Some(18.0),
    });
    let stage = machine_translation(&mut project, &[layer]).await;
    reset_translation_styles(
        &mut project,
        &stage,
        &styles,
        false,
        &renderer,
        |_, _, _, _| Ok(()),
    )
    .await
    .unwrap()
    .unwrap();
    let actual = project
        .snapshot()
        .component::<Typography>(layer)
        .unwrap()
        .unwrap();
    assert_eq!(actual.preferred_font.as_deref(), Some(family.as_str()));
    assert_eq!(actual.font_weight, Some(500));
    assert_eq!(actual.font_style, Some(FontStyle::Italic));
    assert_eq!(actual.size, Some(18.0));
    assert!(!actual.auto_fit);
    assert!(matches!(actual.origin, Origin::Generated(_)));
}

#[tokio::test]
async fn typing_chunks_in_one_editing_session_undo_together() {
    let (mut project, [layer, _]) = fixture(true).await;
    project
        .set_translation(layer, Some("ab".into()))
        .await
        .unwrap();
    let before = project.undo.len();
    for (offset, letter, group) in [(2, "c", 7), (3, "d", 7), (4, "e", 8)] {
        let commit = project
            .replace_text_range(layer, offset, offset, letter.into())
            .await
            .unwrap()
            .unwrap();
        project.record_typing(layer, Some(group), &commit);
    }
    assert_eq!(shown_text(&project.snapshot(), layer).unwrap(), "abcde");
    // Session 7 made one step; session 8 started a new one.
    assert_eq!(project.undo.len(), before + 2);
    project.undo().await.unwrap();
    assert_eq!(shown_text(&project.snapshot(), layer).unwrap(), "abcd");
    project.undo().await.unwrap();
    assert_eq!(shown_text(&project.snapshot(), layer).unwrap(), "ab");
    project.redo().await.unwrap();
    assert_eq!(shown_text(&project.snapshot(), layer).unwrap(), "abcd");
    // After an undo/redo, the next chunk of the same session starts a new step.
    let commit = project
        .replace_text_range(layer, 4, 4, "f".into())
        .await
        .unwrap()
        .unwrap();
    project.record_typing(layer, Some(7), &commit);
    project.undo().await.unwrap();
    assert_eq!(shown_text(&project.snapshot(), layer).unwrap(), "abcd");
}
